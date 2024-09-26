use crate::{ffi::mpv_error_string, CLIENT_NAME};
use anyhow::Error;
use std::ffi::{c_int, CStr};

pub fn log_code(error: c_int) {
    unsafe {
        eprintln!(
            "[{CLIENT_NAME}] {}",
            CStr::from_ptr(mpv_error_string(error)).to_str().unwrap()
        )
    }
}

pub fn log_error(error: &Error) {
    unsafe { eprintln!("[{CLIENT_NAME}] {error}") }
}

// Debug
//
// pub async fn log_to_file(info: &str) -> Result<()> {
//     use crate::mpv::expand_path;
//     use std::path::PathBuf;
//     use tokio::io::AsyncWriteExt;

//     let (options, _) = crate::options::read_options()
//         .map_err(|e| crate::log::log_error(&e))
//         .ok()
//         .flatten()
//         .unwrap_or_default();

//     if !["true", "on", "enable"].contains(&options.log) {
//         return Ok(());
//     }

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
