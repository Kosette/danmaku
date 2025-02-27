use crate::{CLIENT_NAME, ffi::mpv_error_string};
use anyhow::Error;
use std::ffi::{CStr, c_int};

pub fn log_code(error: c_int) {
    unsafe {
        eprintln!(
            "[{}] {}",
            CLIENT_NAME.get().unwrap_or(&"".to_string()),
            CStr::from_ptr(mpv_error_string(error)).to_str().unwrap()
        )
    }
}

pub fn log_error(error: &Error) {
    eprintln!("[{}] {error}", CLIENT_NAME.get().unwrap_or(&"".to_string()))
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
