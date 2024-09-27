use crate::{
    emby::{get_episode_info, get_series_info, EpInfo},
    mpv::osd_message,
    options::{read_options, Filter},
};
use anyhow::{anyhow, Ok, Result};
use hex::encode;
use md5::{Digest, Md5};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::{
    hint,
    io::Read,
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
    use std::result::Result::Ok;

    let episode_id = if !is_http_link(path) {
        let hash = get_localfile_hash(path)?;
        let file_name = get_localfile_name(path);

        get_episode_id_by_hash(&hash, &file_name).await?
    } else {
        let ep_info = get_episode_info(path).await?;

        let file_name = format!("{}.mp4", ep_info.name);
        if ep_info.status {
            let epid = get_episode_id_by_info(ep_info, path).await;

            match epid {
                Ok(p) => p,
                Err(e) => {
                    osd_message(&format!("Error: {}, trying matching with video hash", e));
                    get_episode_id_by_hash(&get_stream_hash(path).await?, &file_name).await?
                }
            }
        } else {
            osd_message("matching with video hash");
            get_episode_id_by_hash(&get_stream_hash(path).await?, &file_name).await?
        }
    };

    let danmaku = CLIENT
        .get(format!(
            "https://api.dandanplay.net/api/v2/comment/{}?withRelated=true",
            episode_id
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
    use std::result::Result::Ok;

    match Url::parse(url) {
        Ok(parsed_url) => parsed_url.scheme() == "http" || parsed_url.scheme() == "https",
        Err(_) => false,
    }
}

// Set Limit of buffer size
const MAX_SIZE: usize = 16 * 1024 * 1024;

async fn get_stream_hash(path: &str) -> Result<String> {
    use futures::StreamExt;

    let response = CLIENT.get(path).send().await?;

    // check status Code
    if !response.status().is_success() {
        eprintln!("Failed to fetch file: {:?}", response.status());
        return Err(anyhow!("Failed to get stream"));
    }

    let mut stream = response.bytes_stream();

    let mut downloaded: usize = 0;

    let mut hasher = Md5::new();

    // Read first 16MiB chunk
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;

        if downloaded + chunk.len() > MAX_SIZE {
            let remaining = MAX_SIZE - downloaded;
            hasher.update(&chunk[..remaining]);

            downloaded += chunk.len();

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

    if downloaded < MAX_SIZE {
        return Err(anyhow!("file too small"));
    }

    Ok(encode(result))
}

async fn get_episode_id_by_hash(hash: &str, file_name: &str) -> Result<usize> {
    let json = json!({
    "fileName":file_name,
    "fileHash":hash,
    "matchMode":"hashAndFileName"
    });

    let res = CLIENT
        .post("https://api.dandanplay.net/api/v2/match")
        .header("Content-Type", "application/json")
        .json(&json)
        .send()
        .await?;

    if !res.status().is_success() {
        return Err(anyhow!("failed to match with hash"));
    }

    let data = res.json::<MatchResponse>().await?;

    if !data.is_matched {
        Err(anyhow!("no matching episode"))
    } else if data.matches.len() == 1 {
        Ok(data.matches[0].episode_id)
    } else {
        Err(anyhow!("multiple matching episodes"))
    }
}

#[derive(Debug, Deserialize)]
struct SearchRes {
    animes: Vec<Anime>,
}

#[derive(Debug, Deserialize)]
struct Anime {
    #[serde(rename = "animeId")]
    anime_id: u64,
    #[serde(rename = "episodeCount")]
    episode_count: u64,
}

// total shit
// shitshitshitshitshitshitshitshitshitshitshit
//
async fn get_episode_id_by_info(ep_info: EpInfo, video_url: &str) -> Result<usize> {
    use std::result::Result::Ok;
    use url::form_urlencoded;

    let ep_type = ep_info.r#type;
    let ep_snum = ep_info.sn_index;
    let ep_num = ep_info.ep_index;
    let sid = ep_info.ss_id;

    let encoded_name: String = form_urlencoded::byte_serialize(ep_info.name.as_bytes()).collect();

    let url = format!(
        "https://api.dandanplay.net/api/v2/search/anime?keyword={}&type={}",
        encoded_name, ep_type
    );

    let res = CLIENT
        .get(url)
        .header("Content-Type", "application/json")
        .send()
        .await?;

    if !res.status().is_success() {
        return Err(anyhow!("failed to search series, try again later"));
    }

    let data = res.json::<SearchRes>().await?;

    if data.animes.is_empty() {
        return Err(anyhow!("no matching episode with info"));
    }

    if ep_type == "ova" && data.animes.len() < ep_num as usize {
        return Err(anyhow!("no matching episode with info"));
    };

    let (mut ani_id, mut ep_id) = (0u64, 0u64);

    if ep_type == "ova" {
        // ova只按照ep_num排序，结果无法预期
        (ani_id, ep_id) = (data.animes[ep_num as usize - 1].anime_id, ep_num);
        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    };

    if ep_type == "movie" {
        // 电影永远只取第一个结果
        (ani_id, ep_id) = (data.animes[0].anime_id, 1u64);
        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    };

    let ep_num_list = get_series_info(video_url, &sid).await?;

    if ep_num_list.is_empty() {
        return Err(anyhow!("no matching episode with info"));
    }

    // 如果季数匹配，则直接返回结果
    if data.animes.len() as u64 == ep_num_list.last().unwrap().0 {
        (ani_id, ep_id) = (data.animes[ep_snum as usize - 1].anime_id, ep_num);
        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    };

    if get_dan_sum(&data.animes, ep_snum)? == get_em_sum(&ep_num_list, ep_snum)? {
        (ani_id, ep_id) = (data.animes[ep_snum as usize - 1].anime_id, ep_num);
        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    }

    if ep_num_list[0].0 != 1 && (ep_snum as u64) != ep_num_list.last().unwrap().0 {
        return Err(anyhow!("need more info, skip"));
    }

    if ep_num_list[0].0 != 1 && (data.animes.len() as u64) < ep_num_list.last().unwrap().0 {
        return Err(anyhow!("need more info, skip"));
    }

    if ep_num_list[0].0 != 1
        && data.animes[ep_snum as usize].episode_count == ep_num_list.last().unwrap().1
    {
        (ani_id, ep_id) = (data.animes[ep_snum as usize].anime_id, ep_num);
        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    }

    if ep_num_list[0].0 != 1
        && data.animes[ep_snum as usize].episode_count
            + data.animes[ep_snum as usize - 1].episode_count
            == ep_num_list.last().unwrap().1
    {
        if ep_num <= data.animes[ep_snum as usize - 1].episode_count {
            (ani_id, ep_id) = (data.animes[ep_snum as usize - 1].anime_id, ep_num);
            return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
        } else {
            (ani_id, ep_id) = (
                data.animes[ep_snum as usize].anime_id,
                ep_num - data.animes[ep_snum as usize - 1].episode_count,
            );
            return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
        }
    }

    if ep_num_list[0].0 != 1 {
        return Err(anyhow!("need more info, skip"));
    }

    if get_dan_sum(&data.animes, data.animes.len() as i64)?
        != get_em_sum(&ep_num_list, ep_num_list.len() as i64)?
    {
        return Err(anyhow!("need more info, skip"));
    }

    // SHIT
    //
    // 求解季数被合并的情况
    if data.animes.len() > ep_num_list.len() {
        let offset = data.animes.len() - ep_num_list.len();

        'outer: for i in 0..=offset {
            if get_dan_sum(&data.animes, ep_snum + i as i64)? == get_em_sum(&ep_num_list, ep_snum)?
            {
                for x in 0..=i {
                    if get_dan_sum(&data.animes, ep_snum - 1 + x as i64)?
                        == get_em_sum(&ep_num_list, ep_snum - 1)?
                    {
                        if i == x {
                            (ani_id, ep_id) =
                                (data.animes[ep_snum as usize - 1 + i].anime_id, ep_num);

                            break 'outer;
                        }

                        if i == x + 1
                            && ep_num <= data.animes[ep_snum as usize - 1 + x].episode_count
                        {
                            (ani_id, ep_id) =
                                (data.animes[ep_snum as usize - 1 + x].anime_id, ep_num);
                            break 'outer;
                        }

                        if i == x + 1
                            && ep_num > data.animes[ep_snum as usize - 1 + x].episode_count
                        {
                            (ani_id, ep_id) = (
                                data.animes[ep_snum as usize + x].anime_id,
                                ep_num - data.animes[ep_snum as usize - 1 + x].episode_count,
                            );
                            break 'outer;
                        }

                        if i == x + 2
                            && ep_num <= data.animes[ep_snum as usize - 1 + x].episode_count
                        {
                            (ani_id, ep_id) =
                                (data.animes[ep_snum as usize - 1 + x].anime_id, ep_num);
                            break 'outer;
                        }

                        if i == x + 2
                            && ep_num
                                <= data.animes[ep_snum as usize - 1 + x].episode_count
                                    + data.animes[ep_snum as usize + x].episode_count
                        {
                            (ani_id, ep_id) = (
                                data.animes[ep_snum as usize + x].anime_id,
                                ep_num - data.animes[ep_snum as usize - 1 + x].episode_count,
                            );
                            break 'outer;
                        }

                        if i == x + 2 {
                            (ani_id, ep_id) = (
                                data.animes[ep_snum as usize + x + 1].anime_id,
                                ep_num
                                    - data.animes[ep_snum as usize - 1 + x].episode_count
                                    - data.animes[ep_snum as usize + x].episode_count,
                            );
                            break 'outer;
                        }

                        return Err(anyhow!("too much results"));
                    }
                }
            }
        }
    }

    // shit
    //
    // 求解季数被拆开的情况
    if data.animes.len() < ep_num_list.len() {
        'outer: for i in 1..=data.animes.len() {
            if get_dan_sum(&data.animes, i as i64)? == get_em_sum(&ep_num_list, ep_snum)? {
                if get_dan_sum(&data.animes, i as i64 - 1)?
                    == get_em_sum(&ep_num_list, ep_snum - 1)?
                {
                    (ani_id, ep_id) = (data.animes[i].anime_id, ep_num);
                    break 'outer;
                }

                if get_dan_sum(&data.animes, i as i64 - 1)?
                    == get_em_sum(&ep_num_list, ep_snum - 2)?
                {
                    (ani_id, ep_id) = (
                        data.animes[i].anime_id,
                        ep_num + ep_num_list[ep_snum as usize - 2].1,
                    );
                    break 'outer;
                }
            }

            if get_dan_sum(&data.animes, i as i64 - 1)? == get_em_sum(&ep_num_list, ep_snum - 1)?
                && get_em_sum(&ep_num_list, ep_snum + 1)? == get_dan_sum(&data.animes, i as i64)?
            {
                (ani_id, ep_id) = (data.animes[i].anime_id, ep_num);
                break 'outer;
            }
        }
    }

    if (ani_id, ep_id) == (0, 0) {
        return Err(anyhow!("not matching episode with info"));
    }

    Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?)
}

// 求dandan返回结果中的前n季集数之和
//
fn get_dan_sum(list: &[Anime], index: i64) -> Result<u64> {
    if index > list.len() as i64 || index < 0 {
        return Err(anyhow!("beyond bound of list"));
    }

    let mut sum = 0;

    for item in list.iter().take(index as usize) {
        sum += item.episode_count;
    }

    Ok(sum)
}

// 求emby前n季集数和
//
fn get_em_sum(list: &[(u64, u64)], index: i64) -> Result<u64> {
    if index > list.len() as i64 || index < 0 {
        return Err(anyhow!("beyond bound of list"));
    }

    let mut sum = 0;

    for item in list.iter().take(index as usize) {
        sum += item.1;
    }

    Ok(sum)
}

fn get_localfile_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.mp4")
        .to_string()
}

fn get_localfile_hash(path: &str) -> Result<String> {
    use std::fs::File;
    use std::path::PathBuf;

    let mut file = File::open(PathBuf::from(path))?;

    let mut buffer = vec![0u8; MAX_SIZE];
    let bytes_read = file.read(&mut buffer)?;

    if bytes_read < MAX_SIZE {
        return Err(anyhow!("file too small"));
    }

    let mut hasher = Md5::new();
    hasher.update(&buffer[..bytes_read]);

    Ok(encode(hasher.finalize()))
}

// Debug
//
// pub async fn log_to_file(info: &str) -> Result<()> {
//     use std::path::PathBuf;
//     use tokio::io::AsyncWriteExt;

//     let path = "~~/files/danmu.log";
//     let log_file = PathBuf::from(expand_path(path)?);

//     let mut file = tokio::fs::OpenOptions::new()
//         .write(true)
//         .append(true)
//         .create(true)
//         .open(log_file)
//         .await?;

//     file.write_all(info.as_bytes()).await?;

//     Ok(())
// }
