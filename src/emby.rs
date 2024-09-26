use super::dandanplay::CLIENT;
use anyhow::{anyhow, Ok, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use url::Url;

pub(crate) struct P3 {
    pub host: String,
    pub item_id: String,
    pub api_key: String,
}

pub(crate) fn extract_params(video_url: &str) -> Result<P3> {
    let url = Url::parse(video_url).unwrap();

    let host = format!(
        "{}://{}",
        url.scheme(),
        url.host_str().expect("host not found")
    );

    // 提取 api_key
    let Some(api_key) = url
        .query_pairs()
        .find(|(key, _)| key == "api_key")
        .map(|(_, value)| value)
    else {
        return Err(anyhow!("api_key not found"));
    };

    let pattern = Regex::new(r"^.*/videos/(\d+)/.*").unwrap();

    // 匹配并提取 item_id
    let item_id = if let Some(captures) = pattern.captures(url.path()) {
        String::from(&captures[1])
    } else {
        return Err(anyhow!("item_id not found"));
    };

    Ok(P3 {
        host,
        item_id,
        api_key: api_key.to_string(),
    })
}

pub(crate) struct EpInfo {
    pub r#type: String,
    pub name: String,
    pub sn_index: i64,
    pub ep_index: u64,
    pub ss_id: u64,
    pub status: bool,
}

impl Default for EpInfo {
    fn default() -> Self {
        Self {
            r#type: "unknown".to_string(),
            name: "unknown".to_string(),
            sn_index: 1,
            ep_index: 1,
            ss_id: 1,
            status: false,
        }
    }
}

pub(crate) async fn get_episode_info(video_url: &str) -> Result<EpInfo> {
    use std::result::Result::Ok;

    let P3 {
        host,
        item_id,
        api_key,
    } = match extract_params(video_url) {
        Ok(p) => p,
        Err(e) => return Err(e),
    };

    let url = format!("{}/emby/Items?Ids={}&reqformat=json", host, item_id);

    let response = CLIENT
        .get(url)
        .header("X-Emby-Token", api_key)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("request episode info failed"));
    }

    let json: Value = response.json().await?;

    if json["Items"][0]["Type"] == "Episode" {
        let series_name = &json["Items"][0]["SeriesName"];
        let season = &json["Items"][0]["ParentIndexNumber"];
        let episode = &json["Items"][0]["IndexNumber"];
        let series_id = &json["Items"][0]["SeriesId"];

        if season == "0" {
            // tmdb将所有非正番归类为S0特别篇，因此无法很好的跟bangumi对接，这里只能图一乐
            Ok(EpInfo {
                r#type: "ova".to_string(),
                name: series_name.to_string(),
                ep_index: episode.as_u64().unwrap_or(1),
                status: true,
                ..Default::default()
            })
        } else {
            Ok(EpInfo {
                r#type: "tvseries".to_string(),
                name: series_name.to_string(),
                sn_index: season.as_i64().unwrap_or(1),
                ep_index: episode.as_u64().unwrap_or(1),
                ss_id: series_id.as_u64().unwrap_or(1),
                status: true,
            })
        }
    } else if json["Items"][0]["Type"] == "Movie" {
        Ok(EpInfo {
            r#type: "movie".to_string(),
            name: json["Items"][0]["Name"].to_string(),
            status: true,
            ..Default::default()
        })
    } else {
        Ok(EpInfo::default())
    }
}

#[derive(Deserialize)]
struct Seasons {
    #[serde(rename = "Items")]
    items: Vec<Season>,
}

#[derive(Deserialize)]
struct Season {
    #[serde(rename = "Id")]
    season_id: u64,
    #[serde(rename = "IndexNumber")]
    season_num: u64,
}

#[derive(Deserialize)]
struct Episodes {
    #[serde(rename = "Items")]
    items: Vec<Episode>,
}

#[derive(Deserialize)]
struct Episode {
    #[serde(rename = "ParentIndexNumber")]
    season_num: u64,
}

/// 获取番剧每季度对应剧集数的数组，排除S0
///
pub(crate) async fn get_series_info(video_url: &str, series_id: u64) -> Result<Vec<u64>> {
    use std::result::Result::Ok;

    let P3 { host, api_key, .. } = match extract_params(video_url) {
        Ok(p) => p,
        Err(e) => return Err(e),
    };

    let seasons_url = format!("{}/emby/Shows/{}/Seasons?reqformat=json", host, series_id);
    let seasons = CLIENT
        .get(seasons_url)
        .header("X-Emby-Token", &api_key)
        .send()
        .await?
        .json::<Seasons>()
        .await?;

    let mut episodes_list: Vec<u64> = Vec::new();

    for season in seasons.items {
        if season.season_num != 0 {
            let sid = season.season_id;

            let episodes_url = format!(
                "{}/emby/Shows/{}/Episodes?SeasonId={}&reqformat=json",
                host, series_id, sid
            );
            let episodes = CLIENT
                .get(episodes_url)
                .header("X-Emby-Token", &api_key)
                .send()
                .await?
                .json::<Episodes>()
                .await?;

            let mut sum = 0;
            for ep in episodes.items {
                if ep.season_num != 0 {
                    sum += 1;
                }
            }

            episodes_list.push(sum);
        }
    }

    Ok(episodes_list)
}
