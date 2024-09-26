use super::dandanplay::CLIENT;
use anyhow::{anyhow, Ok, Result};
use regex::Regex;
use serde::Deserialize;
use url::Url;

#[derive(Debug)]
pub(crate) struct P3 {
    pub host: String,
    pub item_id: String,
    pub api_key: String,
}

pub(crate) fn extract_params(video_url: &str) -> Result<P3> {
    let url = Url::parse(video_url).unwrap();

    // host
    let host = format!(
        "{}://{}",
        url.scheme(),
        url.host_str().expect("host not found")
    );

    // api_key
    let Some(api_key) = url
        .query_pairs()
        .find(|(key, _)| key == "api_key")
        .map(|(_, value)| value)
    else {
        return Err(anyhow!("api_key not found"));
    };

    let pattern = Regex::new(r"^.*/videos/(\d+)/.*").unwrap();

    // item_id
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

#[derive(Debug)]
pub(crate) struct EpInfo {
    pub r#type: String,
    pub name: String,
    pub sn_index: i64,
    pub ep_index: u64,
    pub ss_id: String,
    pub status: bool,
}

impl Default for EpInfo {
    fn default() -> Self {
        Self {
            r#type: "unknown".to_string(),
            name: "unknown".to_string(),
            sn_index: -1,
            ep_index: 0,
            ss_id: "0".to_string(),
            status: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EpData {
    #[serde(rename = "Items")]
    items: Vec<EpDatum>,
}

#[derive(Debug, Deserialize)]
struct EpDatum {
    #[serde(rename = "Type")]
    r#type: String,
    #[serde(default, rename = "SeriesName")]
    s_name: String,
    #[serde(default, rename = "ParentIndexNumber")]
    s_index: i64,
    #[serde(default, rename = "IndexNumber")]
    e_index: u64,
    #[serde(default, rename = "SeriesId")]
    s_id: String,
    #[serde(default, rename = "Name")]
    name: String,
}

impl Default for EpDatum {
    fn default() -> Self {
        Self {
            r#type: "unknown".to_string(),
            s_name: "unknown".to_string(),
            s_index: -1,
            e_index: 0,
            s_id: "0".to_string(),
            name: "unknown".to_string(),
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
        Err(e) => return Err(anyhow!(format!("Error: {}", e))),
    };

    let url = format!("{}/emby/Items?Ids={}&reqformat=json", host, item_id);

    let response = CLIENT
        .get(url)
        .header("X-Emby-Token", api_key)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "fetch episode info error, status: {:?}",
            response.status()
        ));
    }

    let epdata = response.json::<EpData>().await?;

    if epdata.items[0].r#type == "Episode" {
        if epdata.items[0].s_index == 0 {
            Ok(EpInfo {
                r#type: "ova".to_string(),
                name: epdata.items[0].s_name.clone(),
                ep_index: epdata.items[0].e_index,
                status: true,
                ..Default::default()
            })
        } else {
            Ok(EpInfo {
                r#type: "tvseries".to_string(),
                name: epdata.items[0].s_name.clone(),
                sn_index: epdata.items[0].s_index,
                ep_index: epdata.items[0].e_index,
                ss_id: epdata.items[0].s_id.clone(),
                status: true,
            })
        }
    } else if epdata.items[0].r#type == "Movie" {
        Ok(EpInfo {
            r#type: "movie".to_string(),
            name: epdata.items[0].name.clone(),
            status: true,
            ..Default::default()
        })
    } else {
        Ok(EpInfo::default())
    }
}

#[derive(Debug, Deserialize)]
struct Seasons {
    #[serde(rename = "Items")]
    items: Vec<Season>,
}

#[derive(Debug, Deserialize)]
struct Season {
    #[serde(rename = "Id")]
    season_id: String,
    #[serde(rename = "IndexNumber")]
    season_num: u64,
}

#[derive(Debug, Deserialize)]
struct Episodes {
    #[serde(rename = "Items")]
    items: Vec<Episode>,
}

#[derive(Debug, Deserialize)]
struct Episode {
    #[serde(rename = "ParentIndexNumber")]
    season_num: u64,
}

/// a list containing number of episodes of every season except S0
///
pub(crate) async fn get_series_info(video_url: &str, series_id: &str) -> Result<Vec<u64>> {
    use std::result::Result::Ok;

    let P3 { host, api_key, .. } = match extract_params(video_url) {
        Ok(p) => p,
        Err(e) => return Err(anyhow!(format!("Error: {}", e))),
    };

    let seasons_url = format!("{}/emby/Shows/{}/Seasons?reqformat=json", host, series_id);

    let res = CLIENT
        .get(seasons_url)
        .header("X-Emby-Token", &api_key)
        .send()
        .await?;

    if !res.status().is_success() {
        return Err(anyhow!(format!(
            "fetch seasons info error, status: {}",
            res.status()
        )));
    }

    let seasons = res.json::<Seasons>().await?;

    let mut episodes_list: Vec<u64> = Vec::new();

    for season in seasons.items {
        if season.season_num != 0 {
            let sid = season.season_id;

            let episodes_url = format!(
                "{}/emby/Shows/{}/Episodes?SeasonId={}&reqformat=json",
                host, series_id, sid
            );
            let res = CLIENT
                .get(episodes_url)
                .header("X-Emby-Token", &api_key)
                .send()
                .await?;

            if !res.status().is_success() {
                return Err(anyhow!(format!(
                    "fetch episodes info error, status: {}",
                    res.status()
                )));
            }

            let episodes = res.json::<Episodes>().await?;

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
