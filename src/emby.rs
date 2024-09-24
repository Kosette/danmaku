use super::dandanplay::CLIENT;
use anyhow::{anyhow, Ok, Result};
use regex::Regex;
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
        url.host_str().expect("没有找到主机名")
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

use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
// use std::env;

// 构造请求标头
pub async fn construct_headers(api_key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert("X-Emby-Token", HeaderValue::from_str(api_key).unwrap());
    // headers.insert(
    //     "X-Emby-Device-Id",
    //     HeaderValue::from_str(&env::var("DEVICE_ID").unwrap().to_string()).unwrap(),
    // );
    // headers.insert(
    //     "X-Emby-Device-Name",
    //     HeaderValue::from_str(&get_device_name()).unwrap(),
    // );

    headers
}

// fn get_device_name() -> String {
//     // 尝试在类 Unix 系统上获取主机名
//     #[cfg(unix)]
//     {
//         env::var("HOSTNAME").unwrap_or_else(|_| "Unknown".to_string())
//     }

//     // 尝试在 Windows 系统上获取主机名
//     // 如果是 Windows 系统，使用 "COMPUTERNAME" 环境变量
//     #[cfg(windows)]
//     {
//         env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string())
//     }
// }
pub(crate) struct R {
    pub r#type: String,
    pub name: String,
    pub snum: u64,
    pub r#enum: u64,
    pub status: bool,
}

impl Default for R {
    fn default() -> Self {
        Self {
            r#type: "unknown".to_string(),
            name: "unknown".to_string(),
            snum: 1,
            r#enum: 1,
            status: false,
        }
    }
}

pub(crate) async fn get_episode_info(video_url: &str) -> Result<R> {
    use std::result::Result::Ok;

    let P3 {
        host,
        item_id,
        api_key,
    } = match extract_params(video_url) {
        Ok(p) => p,
        Err(_) => return Ok(R::default()),
    };

    let url = format!("{}/emby/Items?Ids={}", host, item_id);

    let response = CLIENT
        .get(url)
        .headers(construct_headers(&api_key).await)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("request chapter info failed"));
    }

    let json: Value = response.json().await?;

    if json["Items"][0]["Type"] == "Episode" {
        let series_name = &json["Items"][0]["SeriesName"];
        let season = &json["Items"][0]["ParentIndexNumber"];
        let episode = &json["Items"][0]["IndexNumber"];
        if season == "0" {
            // tmdb将所有非正番归类为S0特别篇，因此无法很好的跟bangumi对接，这里只能图一乐
            Ok(R {
                r#type: "ova".to_string(),
                name: series_name.to_string(),
                r#enum: episode.as_u64().unwrap_or(1),
                status: true,
                ..Default::default()
            })
        } else {
            Ok(R {
                r#type: "tvseries".to_string(),
                name: series_name.to_string(),
                snum: season.as_u64().unwrap_or(1),
                r#enum: episode.as_u64().unwrap_or(1),
                status: true,
            })
        }
    } else if json["Items"][0]["Type"] == "Movie" {
        Ok(R {
            r#type: "movie".to_string(),
            name: json["Items"][0]["Name"].to_string(),
            status: true,
            ..Default::default()
        })
    } else {
        Ok(R::default())
    }
}
