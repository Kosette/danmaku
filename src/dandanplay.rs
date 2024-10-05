use crate::utils::CLIENT;
use crate::{
    emby::{get_episode_info, get_series_info, EpInfo},
    mpv::osd_message,
    options::{self, Filter},
};
use anyhow::{anyhow, Ok, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{hint, sync::Arc};
use tracing::{error, info};
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

#[derive(Deserialize, Serialize)]
struct Comment {
    p: String,
    m: String,
}

#[derive(Deserialize, Serialize)]
struct CommentResponse {
    comments: Vec<Comment>,
}

impl CommentResponse {
    async fn get(episode_id: usize) -> Result<Self> {
        Ok(CLIENT
            .get(format!(
                "https://api.dandanplay.net/api/v2/comment/{}?withRelated=true",
                episode_id
            ))
            .send()
            .await?
            .json::<CommentResponse>()
            .await?)
    }

    async fn save(&self, episode_id: usize) -> Result<()> {
        use crate::mpv::expand_path;
        use std::path::Path;
        use tokio::io::AsyncWriteExt;

        let encoded: Vec<u8> = bincode::serialize(self)?;
        let path_str = expand_path(&format!("~~/files/danmaku/{}", episode_id))?;
        let path = Path::new(&path_str);

        if !path.parent().expect("no parent dir").exists() {
            std::fs::create_dir_all(path.parent().expect("no parent dir"))?;
        }

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)
            .await?;

        file.write_all(&encoded).await?;

        Ok(())
    }

    async fn load(episode_id: usize) -> Result<Self> {
        use super::mpv::expand_path;
        use std::path::Path;
        use tokio::fs::File;
        use tokio::io::AsyncReadExt;

        let path_str = expand_path(&format!("~~/files/danmaku/{}", episode_id))?;
        let path = Path::new(&path_str);

        let mut file = File::open(path).await?;
        let mut contents = vec![];
        file.read_to_end(&mut contents).await?;

        let comments: CommentResponse = bincode::deserialize(&contents)?;
        Ok(comments)
    }
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

pub async fn get_danmaku(path: &str, filter: Arc<Filter>) -> Result<Vec<Danmaku>> {
    use crate::utils::Linkage;
    use crate::utils::{get_localfile_hash, get_localfile_name, get_stream_hash, is_http_link};
    use std::result::Result::Ok;

    let episode_id = if !is_http_link(path) {
        info!("Now playing non HTTP(s) files");

        let hash = get_localfile_hash(path)?;
        let file_name = get_localfile_name(path);

        get_episode_id_by_hash(&hash, &file_name).await?
    } else {
        let ep_info = get_episode_info(path).await?;

        info!("Now streaming from: {}", path);
        info!("Episode info: {}", ep_info);

        let file_name = ep_info.get_name();
        if ep_info.status {
            let mut linkage = match Linkage::load_from_bincode().await {
                Ok(s) => s,
                Err(_) => Linkage::new(),
            };

            let mut episode_id = 0usize;

            if linkage.items.is_empty() {
                let epid = get_episode_id_by_info(&ep_info, path).await;

                match epid {
                    Ok(p) => episode_id = p,
                    Err(_) => {
                        osd_message("trying matching with video hash");
                        episode_id =
                            get_episode_id_by_hash(&get_stream_hash(path).await?, &file_name)
                                .await?
                    }
                }
                linkage.insert(&ep_info.host, &ep_info.item_id, episode_id);
                linkage.save_as_bincode().await?;
            }

            let epid = linkage.get(&ep_info.host, &ep_info.item_id);

            if epid.is_none() {
                let epid = get_episode_id_by_info(&ep_info, path).await;

                match epid {
                    Ok(p) => episode_id = p,
                    Err(_) => {
                        osd_message("trying matching with video hash");
                        episode_id =
                            get_episode_id_by_hash(&get_stream_hash(path).await?, &file_name)
                                .await?
                    }
                }

                linkage.insert(&ep_info.host, &ep_info.item_id, episode_id);
                linkage.save_as_bincode().await?;
            } else if let Some(id) = epid {
                episode_id = id
            }

            if episode_id == 0usize {
                error!("no matching result");
                return Err(anyhow!("no matching result"));
            }
            episode_id
        } else {
            osd_message("trying matching with video hash");

            get_episode_id_by_hash(&get_stream_hash(path).await?, &file_name).await?
        }
    };

    let danmaku = match CommentResponse::load(episode_id).await {
        Ok(res) => res.comments,
        Err(_) => {
            let comres = CommentResponse::get(episode_id).await?;
            comres.save(episode_id).await?;
            comres.comments
        }
    };

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
        error!("Failed to matching by hash, Status: {:?}", res.status());

        return Err(anyhow!("failed to match with hash"));
    }

    let data = res.json::<MatchResponse>().await?;

    if !data.is_matched {
        error!("No matching result by hash");

        Err(anyhow!("no matching episode"))
    } else if data.matches.len() == 1 {
        info!(
            "Success, matching episode id: {}",
            data.matches[0].episode_id
        );

        Ok(data.matches[0].episode_id)
    } else {
        error!("Too many results");

        Err(anyhow!("multiple matching episodes"))
    }
}

// total shit
// shitshitshitshitshitshitshitshitshitshitshit
//
async fn get_episode_id_by_info(ep_info: &EpInfo, video_url: &str) -> Result<usize> {
    use crate::utils::{get_dan_sum, get_em_sum, SearchRes};
    use std::result::Result::Ok;
    use url::form_urlencoded;

    let encoded_name: String =
        form_urlencoded::byte_serialize(ep_info.get_series_name().as_bytes()).collect();

    let ep_type = &ep_info.r#type;
    let ep_snum = ep_info.sn_index;
    let ep_num = ep_info.ep_index;
    let sid = &ep_info.ss_id;

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
        error!(
            "Failed to searching by keywords, Status: {:?}",
            res.status()
        );

        return Err(anyhow!("failed to search series, try again later"));
    }

    let data = res.json::<SearchRes>().await?;

    if data.animes.is_empty() {
        error!("No matching result");

        return Err(anyhow!("no matching episode with info"));
    }

    if ["true", "on", "enable"].contains(&options::OPTIONS.log) {
        let dandan_search = data
            .animes
            .iter()
            .map(|f| (&f.anime_title, f.episode_count))
            .collect::<Vec<(_, _)>>();

        info!("Search results from Dandanplay: {:?}", dandan_search);
    }

    if ep_type == "ova" && data.animes.len() < ep_num as usize {
        error!("No matching OVA");

        return Err(anyhow!("no matching episode with info"));
    };

    let (mut ani_id, mut ep_id) = (0u64, 0u64);

    if ep_type == "ova" {
        // ova只按照ep_num排序，结果无法预期
        (ani_id, ep_id) = (data.animes[ep_num as usize - 1].anime_id, ep_num);

        info!("Success, ova episode id: {}{:04}", ani_id, ep_id);

        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    };

    if ep_type == "movie" {
        // 电影永远只取第一个结果
        (ani_id, ep_id) = (data.animes[0].anime_id, 1u64);

        info!("Success, movie episode id: {}{:04}", ani_id, ep_id);

        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    };

    let ep_num_list = get_series_info(video_url, sid).await?;

    if ep_num_list.is_empty() {
        error!("Ooops, series info fetching from Emby is empty");

        return Err(anyhow!("no matching episode with info"));
    }

    // 如果季数匹配，则直接返回结果
    if data.animes.len() as u64 == ep_num_list.last().unwrap().0 {
        (ani_id, ep_id) = (data.animes[ep_snum as usize - 1].anime_id, ep_num);

        info!("Success, tv series episode id: {}{:04}", ani_id, ep_id);

        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    };

    if get_dan_sum(&data.animes, ep_snum)? == get_em_sum(&ep_num_list, ep_snum)? {
        (ani_id, ep_id) = (data.animes[ep_snum as usize - 1].anime_id, ep_num);

        info!("Success, tv series episode id: {}{:04}", ani_id, ep_id);

        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    }

    if ep_num_list[0].0 != 1 && (ep_snum as u64) != ep_num_list.last().unwrap().0 {
        error!("Hard to decide, insufficient info");

        return Err(anyhow!("need more info, skip"));
    }

    if ep_num_list[0].0 != 1 && (data.animes.len() as u64) < ep_num_list.last().unwrap().0 {
        error!("Hard to decide, insufficient info");

        return Err(anyhow!("need more info, skip"));
    }

    if ep_num_list[0].0 != 1
        && data.animes[ep_snum as usize].episode_count == ep_num_list.last().unwrap().1
    {
        (ani_id, ep_id) = (data.animes[ep_snum as usize].anime_id, ep_num);
        info!("Success, tv series episode id: {}{:04}", ani_id, ep_id);

        return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
    }

    if ep_num_list[0].0 != 1
        && data.animes[ep_snum as usize].episode_count
            + data.animes[ep_snum as usize - 1].episode_count
            == ep_num_list.last().unwrap().1
    {
        if ep_num <= data.animes[ep_snum as usize - 1].episode_count {
            (ani_id, ep_id) = (data.animes[ep_snum as usize - 1].anime_id, ep_num);
            info!("Success, tv series episode id: {}{:04}", ani_id, ep_id);

            return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
        } else {
            (ani_id, ep_id) = (
                data.animes[ep_snum as usize].anime_id,
                ep_num - data.animes[ep_snum as usize - 1].episode_count,
            );
            info!("Success, tv series episode id: {}{:04}", ani_id, ep_id);

            return Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?);
        }
    }

    if ep_num_list[0].0 != 1 {
        error!("Hard to decide, insufficient info");

        return Err(anyhow!("need more info, skip"));
    }

    if get_dan_sum(&data.animes, data.animes.len() as i64)?
        != get_em_sum(&ep_num_list, ep_num_list.len() as i64)?
    {
        error!("Hard to decide, insufficient info");

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
                        error!("Too many results");

                        return Err(anyhow!("too many results"));
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
        error!("No matching result");
        return Err(anyhow!("not matching episode with info"));
    }
    info!("Success, tv series episode id: {}{:04}", ani_id, ep_id);

    Ok(format!("{}{:04}", ani_id, ep_id).parse::<usize>()?)
}
