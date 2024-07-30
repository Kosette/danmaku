use crate::{danmaku::Source, log::log_error, mpv::expand_path, CLIENT_NAME};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, ErrorKind},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Deserialize)]
struct BilibiliFilterRule {
    r#type: usize,
    filter: String,
    opened: bool,
}

#[derive(Clone, Copy)]
pub struct Options {
    pub font_size: f64,
    pub transparency: u8,
    pub reserved_space: f64,
    pub delay: f64,
}

#[derive(Default)]
pub struct Filter {
    pub keywords: Vec<String>,
    pub sources: HashSet<Source>,
    pub sources_rt: Mutex<Option<HashSet<Source>>>,
}

pub fn read_options() -> (Options, Arc<Filter>) {
    let mut opts = Options {
        font_size: 40.,
        transparency: 0x30,
        reserved_space: 0.,
        delay: 0.,
    };
    let Some(path) = expand_path(&format!("~~/script-opts/{}.conf", unsafe { CLIENT_NAME })) else {
        return (opts, Default::default());
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return (opts, Default::default()),
        Err(error) => {
            log_error(error.into());
            return (opts, Default::default());
        }
    };

    let mut filter = Default::default();
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                log_error(error.into());
                return (opts, Arc::new(filter));
            }
        };
        if !line.starts_with('#') {
            if let Some((k, v)) = line.split_once('=') {
                match k {
                    "font_size" => {
                        if let Some(f) = v.parse().ok().filter(|&f| f > 0.) {
                            opts.font_size = f;
                        }
                    }
                    "transparency" => {
                        if let Ok(t) = v.parse() {
                            opts.transparency = t;
                        }
                    }
                    "reserved_space" => {
                        if let Some(r) = v.parse().ok().filter(|r| (0. ..1.).contains(r)) {
                            opts.reserved_space = r;
                        }
                    }
                    "filter" if !v.is_empty() => {
                        filter.keywords.extend(v.split(',').map(Into::into))
                    }
                    "filter_source" if !v.is_empty() => filter.sources.extend(
                        v.split(',')
                            .map(Source::from)
                            .filter(|&s| s != Source::Unknown),
                    ),
                    "filter_bilibili" if !v.is_empty() => {
                        if let Some(file) = expand_path(v) {
                            match (|| -> Result<_> {
                                Ok(serde_json::from_reader::<_, Vec<BilibiliFilterRule>>(
                                    BufReader::new(File::open(file)?),
                                )?)
                            })() {
                                Ok(rules) => filter.keywords.extend(
                                    rules
                                        .into_iter()
                                        .filter(|r| r.r#type == 0 && r.opened)
                                        .map(|r| r.filter),
                                ),
                                Err(error) => {
                                    log_error(anyhow!("option filter_bilibili: {}", error))
                                }
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
    }
    (opts, Arc::new(filter))
}
