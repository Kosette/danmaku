use crate::{
    emby::get_episode_info,
    // mpv::osd_message,
    options::{read_options, Filter},
};
use anyhow::{anyhow, Result};
use hex::encode;
use md5::{Digest, Md5};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::{
    hint,
    io::{copy, Read},
    sync::{Arc, LazyLock},
};
use unicode_segmentation::UnicodeSegmentation;

pub struct StatusInner {
    pub x: f64,
    pub row: usize,
    pub step: f64,
}

pub enum Status {
    Status(StatusInner),
    Overlapping,
    Uninitialized,
}

impl Status {
    pub fn insert(&mut self, status: StatusInner) -> &mut StatusInner {
        *self = Status::Status(status);
        match self {
            Status::Status(status) => status,
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }
}

pub struct Danmaku {
    pub message: String,
    pub count: usize,
    pub time: f64,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub source: Source,
    pub blocked: bool,
    pub status: Status,
}

#[derive(Deserialize)]
struct MatchResponse {
    #[serde(rename = "isMatched")]
    is_matched: bool,
    matches: Vec<Match>,
}

#[derive(Deserialize)]
struct Match {
    #[serde(rename = "episodeId")]
    episode_id: usize,
}

#[derive(Deserialize)]
struct CommentResponse {
    comments: Vec<Comment>,
}

#[derive(Deserialize)]
struct Comment {
    p: String,
    m: String,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Source {
    Bilibili,
    Gamer,
    AcFun,
    QQ,
    IQIYI,
    D,
    Dandan,
    Unknown,
}

impl From<&str> for Source {
    fn from(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "bilibili" => Source::Bilibili,
            "gamer" => Source::Gamer,
            "acfun" => Source::AcFun,
            "qq" => Source::QQ,
            "iqiyi" => Source::IQIYI,
            "d" => Source::D,
            "dandan" => Source::Dandan,
            _ => Source::Unknown,
        }
    }
}

pub(crate) static CLIENT: LazyLock<Client> = LazyLock::new(build);

fn build() -> reqwest::Client {
    let (options, _) = read_options()
        .map_err(|e| crate::log::log_error(&e))
        .ok()
        .flatten()
        .unwrap_or_default();

    // osd_message(&format!(
    //     "代理: {}\nUA: {}",
    //     options.proxy, options.user_agent
    // ));

    if options.proxy.is_empty() {
        Client::builder()
            .user_agent(options.user_agent)
            .build()
            .expect("Failed to build client")
    } else {
        Client::builder()
            .proxy(reqwest::Proxy::all(options.proxy).unwrap())
            .user_agent(options.user_agent)
            .build()
            .expect("Failed to build client")
    }
}

pub async fn get_danmaku(path: &str, filter: Arc<Filter>) -> Result<Vec<Danmaku>> {
    // use tokio::fs::File;
    // use tokio::io::AsyncReadExt;

    let (hash, file_name) = if is_http_link(path) {
        let hash = get_stream_hash(path).await?;
        let ep_info = get_episode_info(path).await?;

        if !ep_info.is_empty() {
            (hash, format!("{}.mp4", ep_info.trim_matches('"')))
        } else {
            (hash, "original.mp4".to_string())
        }
    } else {
        // let mut file = File::open(path).await?;
        // let mut buffer = vec![0u8; 16 * 1024 * 1024];

        // let bytes_read = file.read(&mut buffer).await?;
        // file.shutdown().await?;

        // buffer.truncate(bytes_read);

        // let mut hasher = Md5::new();
        // hasher.update(&buffer[..]);

        // let result = hasher.finalize();

        let file = std::fs::File::open(std::path::PathBuf::from(path))?;
        let mut hasher = Md5::new();
        copy(&mut file.take(MAX_SIZE as u64), &mut hasher)?;
        let hash = encode(hasher.finalize());

        let file_name = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("original.mp4");

        (hash, file_name.to_string())
    };

    // osd_message(&format!("文件名: {}\n哈希值: {}", file_name, hash));

    // fileName在fileHash匹配失效之后进行模糊匹配，而本脚本不支持非精确结果，故而fileName做了逻辑简化处理
    //
    // 从emby推流链接中提取文件名比较麻烦，且不一定规范，目前拼接为 `名称 S<季>E<集>.mp4`，效果不大
    let json = json!({
    "fileName":&file_name,
    "fileHash":&hash,
    "fileSize":0,
    "videoDuration":0,
    "matchMode":"hashAndFileName"
    });

    let data = CLIENT
        .post("https://api.dandanplay.net/api/v2/match")
        .header("Content-Type", "application/json")
        .json(&json)
        .send()
        .await?
        .json::<MatchResponse>()
        .await?;
    if data.matches.len() > 1 {
        return Err(anyhow!("multiple matching episodes"));
    } else if !data.is_matched {
        return Err(anyhow!("no matching episode"));
    }

    let danmaku = CLIENT
        .get(format!(
            "https://api.dandanplay.net/api/v2/comment/{}?withRelated=true",
            data.matches[0].episode_id
        ))
        .send()
        .await?
        .json::<CommentResponse>()
        .await?
        .comments;
    let sources_rt = filter.sources_rt.lock().await;
    let mut danmaku = danmaku
        .into_iter()
        .filter(|comment| filter.keywords.iter().all(|pat| !comment.m.contains(pat)))
        .map(|comment| {
            let mut p = comment.p.splitn(4, ',');
            let time = p.next().unwrap().parse().unwrap();
            _ = p.next().unwrap();
            let color = p.next().unwrap().parse::<u32>().unwrap();
            let user = p.next().unwrap();
            let source = if user.chars().all(char::is_numeric) {
                Source::Dandan
            } else {
                user.strip_prefix('[')
                    .and_then(|user| user.split_once(']').map(|(source, _)| source.into()))
                    .unwrap_or(Source::Unknown)
            };
            Danmaku {
                message: comment.m.replace('\n', "\\N"),
                count: comment.m.graphemes(true).count(),
                time,
                r: (color / (256 * 256)).try_into().unwrap(),
                g: (color % (256 * 256) / 256).try_into().unwrap(),
                b: (color % 256).try_into().unwrap(),
                source,
                blocked: sources_rt
                    .as_ref()
                    .map(|s| s.contains(&source))
                    .unwrap_or_else(|| filter.sources.contains(&source)),
                status: Status::Uninitialized,
            }
        })
        .collect::<Vec<_>>();

    danmaku.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    Ok(danmaku)
}

use url::Url;

fn is_http_link(url: &str) -> bool {
    match Url::parse(url) {
        Ok(parsed_url) => parsed_url.scheme() == "http" || parsed_url.scheme() == "https",
        Err(_) => false,
    }
}

use futures::StreamExt;

// 设置缓存最大值16MB
const MAX_SIZE: usize = 16 * 1024 * 1024;

async fn get_stream_hash(path: &str) -> Result<String> {
    let response = CLIENT.get(path).send().await?;

    // 检查响应状态码
    if !response.status().is_success() {
        eprintln!("Failed to fetch file: {:?}", response.status());
        return Err(anyhow!("Failed to get stream"));
    }

    // 获取响应的字节流
    let mut stream = response.bytes_stream();

    let mut downloaded: usize = 0;

    let mut hasher = Md5::new();

    // 遍历下载的数据流，只读取前 16M 数据
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;

        if downloaded + chunk.len() > MAX_SIZE {
            let remaining = MAX_SIZE - downloaded;
            hasher.update(&chunk[..remaining]);

            break;
        } else {
            hasher.update(&chunk);

            downloaded += chunk.len();
        }

        if downloaded >= MAX_SIZE {
            break;
        }
    }

    let result = hasher.finalize();

    Ok(encode(result))
}
