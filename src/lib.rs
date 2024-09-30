pub mod dandanplay;
pub mod emby;
pub mod ffi;
pub mod log;
pub mod mpv;
pub mod options;

use crate::{
    dandanplay::{get_danmaku, Danmaku, Source, Status, StatusInner},
    ffi::{
        mpv_client_name, mpv_event_client_message, mpv_event_id, mpv_event_property, mpv_format,
        mpv_handle, mpv_node, mpv_observe_property, mpv_wait_event, mpv_wakeup,
    },
    log::{log_code, log_error},
    mpv::{get_property_f64, get_property_string, osd_message, osd_overlay, remove_overlay},
    options::{Filter, Options},
};
use anyhow::anyhow;
use mpv::expand_path;
use rand::{thread_rng, Rng};
use std::{
    collections::HashSet,
    ffi::CStr,
    fs,
    os::raw::c_int,
    ptr::null_mut,
    slice::from_raw_parts,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
};
use tokio::{runtime::Builder, spawn, sync::Mutex};

use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::time::ChronoUtc;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const MAX_DURATION: f64 = 12.;
const INTERVAL: f64 = 0.005;
const MIN_STEP: f64 = INTERVAL / MAX_DURATION;
const MAX_STEP: f64 = MIN_STEP * 1.3;

pub static mut CTX: *mut mpv_handle = null_mut();
pub static mut CLIENT_NAME: &str = "";

static ENABLED: AtomicBool = AtomicBool::new(false);
static COMMENTS: LazyLock<Mutex<Option<Vec<Danmaku>>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Default, Clone, Copy)]
struct Params {
    delay: f64,
    speed: f64,
    osd_width: f64,
    osd_height: f64,
}

#[no_mangle]
extern "C" fn mpv_open_cplugin(ctx: *mut mpv_handle) -> c_int {
    unsafe {
        CTX = ctx;
        CLIENT_NAME = CStr::from_ptr(mpv_client_name(ctx)).to_str().unwrap();
    }

    Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(main())
}

async fn main() -> c_int {
    for (name, format) in [
        (c"script-opts", mpv_format::MPV_FORMAT_NODE),
        (c"pause", mpv_format::MPV_FORMAT_FLAG),
        (c"speed", mpv_format::MPV_FORMAT_DOUBLE),
        (c"osd-width", mpv_format::MPV_FORMAT_DOUBLE),
        (c"osd-height", mpv_format::MPV_FORMAT_DOUBLE),
    ] {
        let error = unsafe { mpv_observe_property(CTX, 0, name.as_ptr(), format) };
        if error < 0 {
            log_code(error);
            return -1;
        }
    }

    let options = *options::OPTIONS;

    let filter = options::read_options()
        .map_err(|e| log_error(&e))
        .ok()
        .flatten()
        .unwrap_or_default()
        .1;

    // Initialize tracing subscriber
    if ["true", "on", "enable"].contains(&options.log) {
        let log_dir = expand_path("~~/files").expect("can not expand log_dir");

        if !std::path::Path::new(&log_dir).exists() {
            fs::create_dir_all(&log_dir)
                .unwrap_or_else(|_| panic!("create log_dir: {} failed", &log_dir));
        }

        let filter = EnvFilter::from_default_env().add_directive(Level::INFO.into());
        let file_appender = RollingFileAppender::new(Rotation::NEVER, log_dir, "danmu.log");

        let file_layer = fmt::layer()
            .with_writer(file_appender)
            .with_ansi(false)
            .with_timer(ChronoUtc::rfc_3339());

        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .init();
    } else {
        let filter = EnvFilter::from_default_env().add_directive(Level::INFO.into());

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_timer(ChronoUtc::rfc_3339()))
            .init();
    }

    let mut handle = spawn(async {});
    let mut params = Params::default();
    let mut pause = true;
    loop {
        let timeout = if !pause && ENABLED.load(Ordering::SeqCst) {
            INTERVAL
        } else {
            -1.
        };
        let event = unsafe { &*mpv_wait_event(CTX, timeout) };
        match event.event_id {
            mpv_event_id::MPV_EVENT_SHUTDOWN => {
                handle.abort();
                return 0;
            }
            mpv_event_id::MPV_EVENT_FILE_LOADED => {
                handle.abort();
                *COMMENTS.lock().await = None;
                params.delay = 0.;
                if ENABLED.load(Ordering::SeqCst) {
                    remove_overlay();
                    handle = spawn(get(filter.clone()));
                }
            }
            mpv_event_id::MPV_EVENT_PLAYBACK_RESTART => {
                if ENABLED.load(Ordering::SeqCst) {
                    if let Some(comments) = &mut *COMMENTS.lock().await {
                        reset_status(comments);
                        render(comments, params, options);
                    }
                }
            }
            mpv_event_id::MPV_EVENT_PROPERTY_CHANGE => 'a: {
                let data = unsafe { &*(event.data as *mut mpv_event_property) };
                if data.format == mpv_format::MPV_FORMAT_NONE {
                    break 'a;
                }
                let name = unsafe { CStr::from_ptr(data.name) };
                if name == c"pause" {
                    pause = unsafe { *(data.data as *mut c_int) } != 0;
                } else if name == c"osd-width" {
                    params.osd_width = unsafe { *(data.data as *mut f64) };
                } else if name == c"osd-height" {
                    params.osd_height = unsafe { *(data.data as *mut f64) };
                } else if name == c"script-opts" {
                    let data = unsafe { &*(data.data as *mut mpv_node) };
                    assert_eq!(data.format, mpv_format::MPV_FORMAT_NODE_MAP);
                    let list = unsafe { &*data.u.list };
                    if list.num == 0 {
                        break 'a;
                    }
                    let num = list.num.try_into().unwrap();
                    let keys = unsafe { from_raw_parts(list.keys, num) };
                    let values = unsafe { from_raw_parts(list.values, num) };
                    for (key, value) in keys.iter().zip(values) {
                        if unsafe { CStr::from_ptr(key.cast()) }
                            .to_str()
                            .is_ok_and(|key| {
                                key == format!("{}-filter_source", unsafe { CLIENT_NAME })
                            })
                        {
                            assert_eq!(value.format, mpv_format::MPV_FORMAT_STRING);
                            match unsafe { CStr::from_ptr(value.u.string) }.to_str() {
                                Ok(value) => {
                                    *filter.sources_rt.lock().await = if value.is_empty() {
                                        if let Some(comments) = &mut *COMMENTS.lock().await {
                                            for comment in comments.iter_mut() {
                                                comment.blocked =
                                                    filter.sources.contains(&comment.source);
                                                comment.status = Status::Uninitialized;
                                            }
                                            if ENABLED.load(Ordering::SeqCst) {
                                                render(comments, params, options);
                                            }
                                        }
                                        osd_message(&format!(
                                            "Danmaku: blocked danmaku from {:?}",
                                            filter.sources
                                        ));
                                        None
                                    } else {
                                        let sources = value
                                            .split(',')
                                            .map(Into::into)
                                            .filter(|&s| s != Source::Unknown)
                                            .collect::<HashSet<_>>();
                                        if let Some(comments) = &mut *COMMENTS.lock().await {
                                            for comment in comments.iter_mut() {
                                                comment.blocked = sources.contains(&comment.source);
                                                comment.status = Status::Uninitialized;
                                            }
                                            if ENABLED.load(Ordering::SeqCst) {
                                                render(comments, params, options);
                                            }
                                        }
                                        osd_message(&format!(
                                            "Danmaku: blocked danmaku from {:?}",
                                            sources
                                        ));
                                        Some(sources)
                                    }
                                }
                                Err(error) => log_error(&error.into()),
                            }
                            break;
                        }
                    }
                } else if name == c"speed" {
                    params.speed = unsafe { *(data.data as *mut f64) };
                }
            }
            mpv_event_id::MPV_EVENT_CLIENT_MESSAGE => 'a: {
                let data = unsafe { &*(event.data as *mut mpv_event_client_message) };
                if data.args.is_null() {
                    break 'a;
                }
                if let [arg1, args @ ..] =
                    unsafe { from_raw_parts(data.args, data.num_args.try_into().unwrap()) }
                {
                    let arg1 = unsafe { CStr::from_ptr(*arg1) };
                    if arg1 == c"toggle-danmaku" {
                        if ENABLED.fetch_not(Ordering::SeqCst) {
                            handle.abort();
                            remove_overlay();
                            osd_message("Danmaku: off");
                        } else {
                            match &mut *COMMENTS.lock().await {
                                Some(comments) => {
                                    reset_status(comments);
                                    render(comments, params, options);
                                    loaded(comments.iter().filter(|c| !c.blocked).count());
                                }
                                None => {
                                    handle = spawn(get(filter.clone()));
                                    osd_message("Danmaku: on");
                                }
                            }
                        }
                    } else if arg1 == c"danmaku-delay" {
                        match args.first() {
                            Some(&seconds) => {
                                match unsafe { CStr::from_ptr(seconds) }
                                    .to_str()
                                    .ok()
                                    .and_then(|s| s.parse::<f64>().ok())
                                {
                                    Some(seconds) => {
                                        params.delay += seconds;
                                        if ENABLED.load(Ordering::SeqCst) {
                                            if let Some(comments) = &mut *COMMENTS.lock().await {
                                                reset_status(comments);
                                                render(comments, params, options);
                                            }
                                        }
                                        osd_message(&format!(
                                            "Danmaku delay: {:.0} ms",
                                            params.delay * 1000.
                                        ));
                                    }
                                    None => {
                                        log_error(&anyhow!("command danmaku-delay: invalid time"))
                                    }
                                }
                            }
                            None => log_error(&anyhow!(
                                "command danmaku-delay: required argument seconds not set"
                            )),
                        }
                    }
                }
            }
            mpv_event_id::MPV_EVENT_NONE => {
                if let Some(comments) = &mut *COMMENTS.lock().await {
                    render(comments, params, options);
                }
            }
            _ => (),
        }
    }
}

#[derive(Clone, Copy)]
struct Row {
    end: f64,
    step: f64,
}

fn render(comments: &mut [Danmaku], params: Params, options: Options) {
    let Some(pos) = get_property_f64(c"time-pos") else {
        return;
    };
    let mut width = 1920.;
    let mut height = 1080.;
    let ratio = params.osd_width / params.osd_height;
    if width / height < ratio {
        height = width / ratio;
    } else if width / height > ratio {
        width = height * ratio;
    }
    let spacing = options.font_size / 10.;
    let mut rows = vec![
        Row {
            end: 0.,
            step: MIN_STEP,
        };
        ((height * (1. - options.reserved_space) / (options.font_size + spacing))
            as usize)
            .max(1)
    ];

    let mut danmaku = Vec::new();
    let mut rng = thread_rng();
    'it: for comment in comments.iter_mut().filter(|c| !c.blocked) {
        let time = comment.time + params.delay;
        if time > pos {
            break;
        }

        let status = match &mut comment.status {
            Status::Status(status) => status,
            Status::Overlapping => continue,
            Status::Uninitialized => 'status: {
                let ticks = (pos - time) / INTERVAL;
                for (row, status) in rows.iter().enumerate() {
                    if status.end < width - width * ticks * MIN_STEP {
                        let max_step = if status.end == 0. {
                            MAX_STEP
                        } else {
                            // 1 / max_step - ticks = status.end / width / status.step
                            let max_step = 1. / (ticks + status.end / width / status.step);
                            max_step.min(MAX_STEP)
                        };
                        let step = rng.gen_range(MIN_STEP..max_step);
                        let x = width - width * ticks * step;
                        break 'status comment.status.insert(StatusInner { x, row, step });
                    }
                }
                if options.no_overlap {
                    comment.status = Status::Overlapping;
                    continue 'it;
                }
                let row = rows
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.end.partial_cmp(&b.1.end).unwrap())
                    .map(|(row, _)| row)
                    .unwrap();
                let step = MIN_STEP;
                let x = width - width * ticks * step;
                comment.status.insert(StatusInner { x, row, step })
            }
        };
        if status.x + comment.count as f64 * options.font_size + spacing <= 0. {
            continue;
        }
        danmaku.push(format!(
            "{{\\pos({},{})\\c&H{:x}{:x}{:x}&\\alpha&H{:x}\\fs{}\\bord1.5\\shad0\\b1\\q2}}{}",
            status.x,
            status.row as f64 * (options.font_size + spacing),
            comment.b,
            comment.g,
            comment.r,
            options.transparency,
            options.font_size,
            comment.message
        ));

        status.x -= width * status.step * params.speed * options.speed;
        if let Some(row) = rows.get_mut(status.row) {
            let end = status.x + comment.count as f64 * options.font_size + spacing;
            if end / status.step > row.end / row.step {
                *row = Row {
                    end,
                    step: status.step,
                };
            }
        }
    }
    osd_overlay(&danmaku.join("\n"), width as i64, height as i64);
}

async fn get(filter: Arc<Filter>) {
    let Some(path) = get_property_string(c"path") else {
        return;
    };

    match get_danmaku(&path, filter).await {
        Ok(danmaku) => {
            let n = danmaku.iter().filter(|c| !c.blocked).count();
            *COMMENTS.lock().await = Some(danmaku);
            if ENABLED.load(Ordering::SeqCst) {
                unsafe { mpv_wakeup(CTX) };
                loaded(n);
            }
        }
        Err(error) => {
            log_error(&error);
            if ENABLED.load(Ordering::SeqCst) {
                osd_message(&format!("Danmaku: {}", error));
            }
        }
    }
}

fn reset_status(comments: &mut [Danmaku]) {
    for comment in comments {
        comment.status = Status::Uninitialized;
    }
}

fn loaded(n: usize) {
    osd_message(&format!(
        "Loaded {} danmaku comment{}",
        n,
        if n > 1 { "s" } else { "" }
    ));
}
