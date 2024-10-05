use crate::options;
use anyhow::{anyhow, Result};
use hex::encode;
use md5::{Digest, Md5};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::LazyLock,
};
use tracing::{error, info};

pub(crate) static CLIENT: LazyLock<Client> = LazyLock::new(build);

fn build() -> reqwest::Client {
    let options = *options::OPTIONS;

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

pub fn is_http_link(url: &str) -> bool {
    use std::result::Result::Ok;
    use url::Url;

    match Url::parse(url) {
        Ok(parsed_url) => parsed_url.scheme() == "http" || parsed_url.scheme() == "https",
        Err(_) => false,
    }
}

// Set Limit of buffer size
const MAX_SIZE: usize = 16 * 1024 * 1024;

pub async fn get_stream_hash(path: &str) -> Result<String> {
    use futures::StreamExt;

    let response = CLIENT.get(path).send().await?;

    // check status Code
    if !response.status().is_success() {
        error!(
            "Failed to fetch data from server, Status: {}",
            response.status()
        );

        return Err(anyhow!("Failed to fetch data from server"));
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
        error!("File too small, less than 16MiB");
        return Err(anyhow!("file too small"));
    }

    info!("Get streaming file hash: {}", encode(result));

    Ok(encode(result))
}

#[derive(Debug, Deserialize)]
pub struct SearchRes {
    pub animes: Vec<Anime>,
}

#[derive(Debug, Deserialize)]
pub struct Anime {
    #[serde(rename = "animeId")]
    pub anime_id: u64,
    #[serde(rename = "episodeCount")]
    pub episode_count: u64,
    #[serde(rename = "animeTitle")]
    pub anime_title: String,
}

// 求dandan返回结果中的前n季集数之和
//
pub fn get_dan_sum(list: &[Anime], index: i64) -> Result<u64> {
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
pub fn get_em_sum(list: &[(u64, u64)], index: i64) -> Result<u64> {
    if index > list.len() as i64 || index < 0 {
        return Err(anyhow!("beyond bound of list"));
    }

    let mut sum = 0;
    for item in list.iter().take(index as usize) {
        sum += item.1;
    }

    Ok(sum)
}

pub fn get_localfile_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.mp4")
        .to_string()
}

pub fn get_localfile_hash(path: &str) -> Result<String> {
    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;

    let mut file = File::open(PathBuf::from(path))?;
    let mut buffer = vec![0u8; MAX_SIZE];
    let bytes_read = file.read(&mut buffer)?;

    if bytes_read < MAX_SIZE {
        error!("File too small, less than 16MiB");
        return Err(anyhow!("file too small"));
    }

    let mut hasher = Md5::new();
    hasher.update(&buffer[..bytes_read]);

    Ok(encode(hasher.finalize()))
}

use std::time::{Duration, SystemTime};

#[derive(Debug, Serialize, Deserialize)]
pub struct TimesId {
    pub epid: usize,
    last_updated: SystemTime,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Linkage {
    pub items: HashMap<String, LimitedHashMap<String, TimesId>>,
    pub seasons: HashMap<String, LimitedHashMap<String, usize>>,
}

impl Default for Linkage {
    fn default() -> Self {
        Self::new()
    }
}

impl Linkage {
    pub fn new() -> Self {
        Linkage {
            items: HashMap::new(),
            seasons: HashMap::new(),
        }
    }

    pub fn insert_items(&mut self, host_key: &str, item_id: &str, epid: usize) {
        let timestamped_value = TimesId {
            epid,
            last_updated: SystemTime::now(),
        };
        self.items
            .entry(host_key.to_string())
            .or_default()
            .insert(item_id.to_string(), timestamped_value);
    }

    pub fn get_items(&self, host_key: &str, item_id: &str) -> Option<usize> {
        self.items.get(host_key)?.get(item_id).map(|tv| tv.epid)
    }

    pub fn insert_seasons(&mut self, host_key: &str, season_id: &str, anime_id: usize) {
        self.seasons
            .entry(host_key.to_string())
            .or_default()
            .insert(season_id.to_string(), anime_id);
    }

    pub fn get_seasons(&self, host_key: &str, season_id: &str) -> Option<usize> {
        self.seasons.get(host_key)?.get(season_id).copied()
    }

    pub fn clean_expired_entries(&mut self, expiration_duration: Duration) {
        let now = SystemTime::now();
        self.items.retain(|_, inner_map| {
            inner_map.map.retain(|_, timestamped_value| {
                now.duration_since(timestamped_value.last_updated)
                    .map(|age| age < expiration_duration)
                    .unwrap_or(true)
            });
            !inner_map.is_empty()
        });
    }

    pub async fn save_as_bincode(&self) -> Result<()> {
        use crate::mpv::expand_path;
        use std::path::Path;
        use tokio::io::AsyncWriteExt;

        let encoded: Vec<u8> = bincode::serialize(self)?;
        let path_str = expand_path("~~/files/danmaku/database")?;
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

    pub async fn load_from_bincode() -> Result<Self> {
        use super::mpv::expand_path;
        use std::path::Path;
        use tokio::fs::File;
        use tokio::io::AsyncReadExt;

        let path_str = expand_path("~~/files/danmaku/database")?;
        let path = Path::new(&path_str);

        let mut file = File::open(path).await?;
        let mut contents = vec![];

        file.read_to_end(&mut contents).await?;

        let linkage: Linkage = bincode::deserialize(&contents)?;
        Ok(linkage)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LimitedHashMap<K, V>
where
    K: Clone + std::hash::Hash + Eq,
{
    map: HashMap<K, V>,
    keys: VecDeque<K>,
    capacity: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V> Default for LimitedHashMap<K, V> {
    fn default() -> Self {
        Self::new(30)
    }
}

impl<K: std::hash::Hash + Eq + Clone, V> LimitedHashMap<K, V> {
    fn new(capacity: usize) -> Self {
        LimitedHashMap {
            map: HashMap::new(),
            keys: VecDeque::new(),
            capacity,
        }
    }

    fn insert(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            self.map.insert(key.clone(), value);
        } else {
            if self.keys.len() == self.capacity {
                if let Some(oldest_key) = self.keys.pop_front() {
                    self.map.remove(&oldest_key);
                }
            }

            self.keys.push_back(key.clone());
            self.map.insert(key, value);
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        Q: ?Sized,
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.get(key)
    }

    fn _len(&self) -> usize {
        self.map.len()
    }

    fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
