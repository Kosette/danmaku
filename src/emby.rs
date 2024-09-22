use super::dandanplay::CLIENT;
use anyhow::{anyhow, Result};
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

pub async fn get_episode_info(video_url: &str) -> Result<String> {
    let P3 {
        host,
        item_id,
        api_key,
    } = match extract_params(video_url) {
        Ok(p) => p,
        Err(_) => return Ok("".to_string()),
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
        Ok(format!(
            "{} S{}E{}",
            series_name.to_string().trim_matches('"'),
            season.to_string().trim_matches('"'),
            episode.to_string().trim_matches('"')
        ))
    } else if json["Items"][0]["Type"] == "Movie" {
        Ok(json["Items"][0]["Name"].to_string())
    } else {
        Ok("".to_string())
    }
}
