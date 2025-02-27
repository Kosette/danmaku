#![allow(clippy::uninit_assumed_init)]
#![allow(invalid_value)]

use crate::{
    CTX,
    ffi::{
        mpv_command, mpv_command_node, mpv_command_ret, mpv_error_string, mpv_format, mpv_free,
        mpv_free_node_contents, mpv_get_property, mpv_node, mpv_node_list, u,
    },
    log_code,
};
use anyhow::{Result, anyhow};
use std::{
    ffi::{CStr, CString, c_char},
    mem::MaybeUninit,
    ptr::{addr_of_mut, null, null_mut},
};

pub fn osd_overlay(data: &str, width: i64, height: i64) {
    let mut keys = [c"name", c"id", c"format", c"data", c"res_x", c"res_y"]
        .map(|key| CString::from(key).into_raw());
    let value1 = CString::from(c"osd-overlay").into_raw();
    let value3 = CString::from(c"ass-events").into_raw();
    let value4 = CString::new(data).unwrap().into_raw();
    let mut values = [
        mpv_node {
            format: mpv_format::MPV_FORMAT_STRING,
            u: u { string: value1 },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_INT64,
            u: u { int64: 0 },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_STRING,
            u: u { string: value3 },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_STRING,
            u: u { string: value4 },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_INT64,
            u: u { int64: width },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_INT64,
            u: u { int64: height },
        },
    ];
    assert_eq!(keys.len(), values.len());

    let mut list = mpv_node_list {
        num: keys.len().try_into().unwrap(),
        values: values.as_mut_ptr(),
        keys: keys.as_mut_ptr(),
    };
    let mut args = mpv_node {
        format: mpv_format::MPV_FORMAT_NODE_MAP,
        u: u {
            list: addr_of_mut!(list),
        },
    };
    let error = unsafe { mpv_command_node(CTX, addr_of_mut!(args), null_mut()) };
    if error < 0 {
        log_code(error);
    }

    unsafe {
        _ = keys.map(|key| CString::from_raw(key));
        _ = CString::from_raw(value1);
        _ = CString::from_raw(value3);
        _ = CString::from_raw(value4);
    }
}

pub fn remove_overlay() {
    let mut keys = [c"name", c"id", c"format", c"data"].map(|key| CString::from(key).into_raw());
    let value1 = CString::from(c"osd-overlay").into_raw();
    let value3 = CString::from(c"none").into_raw();
    let value4 = CString::from(c"").into_raw();
    let mut values = [
        mpv_node {
            format: mpv_format::MPV_FORMAT_STRING,
            u: u { string: value1 },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_INT64,
            u: u { int64: 0 },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_STRING,
            u: u { string: value3 },
        },
        mpv_node {
            format: mpv_format::MPV_FORMAT_STRING,
            u: u { string: value4 },
        },
    ];
    assert_eq!(keys.len(), values.len());

    let mut list = mpv_node_list {
        num: keys.len().try_into().unwrap(),
        values: values.as_mut_ptr(),
        keys: keys.as_mut_ptr(),
    };
    let mut args = mpv_node {
        format: mpv_format::MPV_FORMAT_NODE_MAP,
        u: u {
            list: addr_of_mut!(list),
        },
    };
    let error = unsafe { mpv_command_node(CTX, addr_of_mut!(args), null_mut()) };
    if error < 0 {
        log_code(error);
    }

    unsafe {
        _ = keys.map(|key| CString::from_raw(key));
        _ = CString::from_raw(value1);
        _ = CString::from_raw(value3);
        _ = CString::from_raw(value4);
    }
}

pub fn get_property_f64(name: &CStr) -> Option<f64> {
    let mut data = unsafe { MaybeUninit::<f64>::uninit().assume_init() };
    let error = unsafe {
        mpv_get_property(
            CTX,
            name.as_ptr(),
            mpv_format::MPV_FORMAT_DOUBLE,
            addr_of_mut!(data).cast(),
        )
    };
    if error < 0 {
        log_code(error);
        None
    } else {
        Some(data)
    }
}

pub fn get_property_string(name: &CStr) -> Option<String> {
    let mut data = unsafe { MaybeUninit::<*mut c_char>::uninit().assume_init() };
    let error = unsafe {
        mpv_get_property(
            CTX,
            name.as_ptr(),
            mpv_format::MPV_FORMAT_STRING,
            addr_of_mut!(data).cast(),
        )
    };
    if error < 0 {
        log_code(error);
        None
    } else {
        let value = unsafe { CStr::from_ptr(data) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { mpv_free(data.cast()) };
        Some(value)
    }
}

pub fn expand_path(path: &str) -> Result<String> {
    unsafe {
        let arg2 = CString::new(path).unwrap();
        let mut args = [c"expand-path".as_ptr(), arg2.as_ptr(), null()];
        let mut result = MaybeUninit::<mpv_node>::uninit().assume_init();
        let error = mpv_command_ret(CTX, args.as_mut_ptr(), addr_of_mut!(result));
        if error < 0 {
            return Err(anyhow!(
                "{}",
                CStr::from_ptr(mpv_error_string(error)).to_str().unwrap()
            ));
        }
        assert_eq!(result.format, mpv_format::MPV_FORMAT_STRING);
        let path = CStr::from_ptr(result.u.string)
            .to_str()
            .unwrap()
            .to_string();
        mpv_free_node_contents(addr_of_mut!(result));
        Ok(path)
    }
}

pub fn osd_message(text: &str) {
    let arg2 = CString::new(text).unwrap();
    let mut args = [c"show-text".as_ptr(), arg2.as_ptr(), null()];
    let error = unsafe { mpv_command(CTX, args.as_mut_ptr()) };
    if error < 0 {
        log_code(error);
    }
}
