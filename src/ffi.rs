#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mpv_handle {
    _unused: [u8; 0],
}

#[repr(i32)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum mpv_error {
    MPV_ERROR_SUCCESS = 0,
    MPV_ERROR_EVENT_QUEUE_FULL = -1,
    MPV_ERROR_NOMEM = -2,
    MPV_ERROR_UNINITIALIZED = -3,
    MPV_ERROR_INVALID_PARAMETER = -4,
    MPV_ERROR_OPTION_NOT_FOUND = -5,
    MPV_ERROR_OPTION_FORMAT = -6,
    MPV_ERROR_OPTION_ERROR = -7,
    MPV_ERROR_PROPERTY_NOT_FOUND = -8,
    MPV_ERROR_PROPERTY_FORMAT = -9,
    MPV_ERROR_PROPERTY_UNAVAILABLE = -10,
    MPV_ERROR_PROPERTY_ERROR = -11,
    MPV_ERROR_COMMAND = -12,
    MPV_ERROR_LOADING_FAILED = -13,
    MPV_ERROR_AO_INIT_FAILED = -14,
    MPV_ERROR_VO_INIT_FAILED = -15,
    MPV_ERROR_NOTHING_TO_PLAY = -16,
    MPV_ERROR_UNKNOWN_FORMAT = -17,
    MPV_ERROR_UNSUPPORTED = -18,
    MPV_ERROR_NOT_IMPLEMENTED = -19,
    MPV_ERROR_GENERIC = -20,
}

#[repr(i32)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum mpv_format {
    MPV_FORMAT_NONE = 0,
    MPV_FORMAT_STRING = 1,
    MPV_FORMAT_OSD_STRING = 2,
    MPV_FORMAT_FLAG = 3,
    MPV_FORMAT_INT64 = 4,
    MPV_FORMAT_DOUBLE = 5,
    MPV_FORMAT_NODE = 6,
    MPV_FORMAT_NODE_ARRAY = 7,
    MPV_FORMAT_NODE_MAP = 8,
    MPV_FORMAT_BYTE_ARRAY = 9,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct mpv_node {
    pub u: u,
    pub format: mpv_format,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union u {
    pub string: *mut c_char,
    pub flag: c_int,
    pub int64: i64,
    pub double_: f64,
    pub list: *mut mpv_node_list,
    pub ba: *mut mpv_byte_array,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mpv_node_list {
    pub num: c_int,
    pub values: *mut mpv_node,
    pub keys: *mut *mut c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mpv_byte_array {
    pub data: *mut c_void,
    pub size: usize,
}

#[repr(i32)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum mpv_event_id {
    MPV_EVENT_NONE = 0,
    MPV_EVENT_SHUTDOWN = 1,
    MPV_EVENT_LOG_MESSAGE = 2,
    MPV_EVENT_GET_PROPERTY_REPLY = 3,
    MPV_EVENT_SET_PROPERTY_REPLY = 4,
    MPV_EVENT_COMMAND_REPLY = 5,
    MPV_EVENT_START_FILE = 6,
    MPV_EVENT_END_FILE = 7,
    MPV_EVENT_FILE_LOADED = 8,
    MPV_EVENT_IDLE = 11,
    MPV_EVENT_TICK = 14,
    MPV_EVENT_CLIENT_MESSAGE = 16,
    MPV_EVENT_VIDEO_RECONFIG = 17,
    MPV_EVENT_AUDIO_RECONFIG = 18,
    MPV_EVENT_SEEK = 20,
    MPV_EVENT_PLAYBACK_RESTART = 21,
    MPV_EVENT_PROPERTY_CHANGE = 22,
    MPV_EVENT_QUEUE_OVERFLOW = 24,
    MPV_EVENT_HOOK = 25,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mpv_event_client_message {
    pub num_args: c_int,
    pub args: *mut *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mpv_event_property {
    pub name: *const c_char,
    pub format: mpv_format,
    pub data: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mpv_event {
    pub event_id: mpv_event_id,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

#[cfg(not(target_os = "windows"))]
extern "C" {
    pub fn mpv_error_string(error: c_int) -> *const c_char;
    pub fn mpv_free(data: *mut c_void);
    pub fn mpv_client_name(ctx: *mut mpv_handle) -> *const c_char;
    pub fn mpv_free_node_contents(node: *mut mpv_node);
    pub fn mpv_command(ctx: *mut mpv_handle, args: *mut *const c_char) -> c_int;
    pub fn mpv_command_node(
        ctx: *mut mpv_handle,
        args: *mut mpv_node,
        result: *mut mpv_node,
    ) -> c_int;
    pub fn mpv_command_ret(
        ctx: *mut mpv_handle,
        args: *mut *const c_char,
        result: *mut mpv_node,
    ) -> c_int;
    pub fn mpv_get_property(
        ctx: *mut mpv_handle,
        name: *const c_char,
        format: mpv_format,
        data: *mut c_void,
    ) -> c_int;
    pub fn mpv_observe_property(
        ctx: *mut mpv_handle,
        reply_userdata: u64,
        name: *const c_char,
        format: mpv_format,
    ) -> c_int;
    pub fn mpv_command_string(ctx: *mut mpv_handle, args: *const c_char) -> c_int;
    pub fn mpv_event_name(event: mpv_event_id) -> *const c_char;
    pub fn mpv_wait_event(ctx: *mut mpv_handle, timeout: f64) -> *mut mpv_event;
    pub fn mpv_wakeup(ctx: *mut mpv_handle);
}

#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_error_string: Option<extern "C" fn(error: c_int) -> *const c_char> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_free: Option<extern "C" fn(data: *mut c_void)> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_client_name: Option<extern "C" fn(ctx: *mut mpv_handle) -> *const c_char> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_free_node_contents: Option<extern "C" fn(node: *mut mpv_node)> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_command: Option<
    extern "C" fn(ctx: *mut mpv_handle, args: *mut *const c_char) -> c_int,
> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_command_node: Option<
    extern "C" fn(ctx: *mut mpv_handle, args: *mut mpv_node, result: *mut mpv_node) -> c_int,
> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_command_ret: Option<
    extern "C" fn(ctx: *mut mpv_handle, args: *mut *const c_char, result: *mut mpv_node) -> c_int,
> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_get_property: Option<
    extern "C" fn(
        ctx: *mut mpv_handle,
        name: *const c_char,
        format: mpv_format,
        data: *mut c_void,
    ) -> c_int,
> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_observe_property: Option<
    extern "C" fn(
        ctx: *mut mpv_handle,
        reply_userdata: u64,
        name: *const c_char,
        format: mpv_format,
    ) -> c_int,
> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_command_string: Option<
    extern "C" fn(ctx: *mut mpv_handle, args: *const c_char) -> c_int,
> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
static mut pfn_mpv_event_name: Option<extern "C" fn(event: mpv_event_id) -> *const c_char> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
pub static mut pfn_mpv_wait_event: Option<
    extern "C" fn(ctx: *mut mpv_handle, timeout: f64) -> *mut mpv_event,
> = None;
#[cfg(target_os = "windows")]
#[no_mangle]
pub static mut pfn_mpv_wakeup: Option<extern "C" fn(ctx: *mut mpv_handle)> = None;

#[cfg(target_os = "windows")]
pub unsafe fn mpv_error_string(error: c_int) -> *const c_char {
    pfn_mpv_error_string.unwrap()(error)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_free(data: *mut c_void) {
    pfn_mpv_free.unwrap()(data)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_client_name(ctx: *mut mpv_handle) -> *const c_char {
    pfn_mpv_client_name.unwrap()(ctx)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_free_node_contents(node: *mut mpv_node) {
    pfn_mpv_free_node_contents.unwrap()(node)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_command(ctx: *mut mpv_handle, args: *mut *const c_char) -> c_int {
    pfn_mpv_command.unwrap()(ctx, args)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_command_node(
    ctx: *mut mpv_handle,
    args: *mut mpv_node,
    result: *mut mpv_node,
) -> c_int {
    pfn_mpv_command_node.unwrap()(ctx, args, result)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_command_ret(
    ctx: *mut mpv_handle,
    args: *mut *const c_char,
    result: *mut mpv_node,
) -> c_int {
    pfn_mpv_command_ret.unwrap()(ctx, args, result)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_get_property(
    ctx: *mut mpv_handle,
    name: *const c_char,
    format: mpv_format,
    data: *mut c_void,
) -> c_int {
    pfn_mpv_get_property.unwrap()(ctx, name, format, data)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_observe_property(
    ctx: *mut mpv_handle,
    reply_userdata: u64,
    name: *const c_char,
    format: mpv_format,
) -> c_int {
    pfn_mpv_observe_property.unwrap()(ctx, reply_userdata, name, format)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_command_string(ctx: *mut mpv_handle, args: *const c_char) -> c_int {
    pfn_mpv_command_string.unwrap()(ctx, args)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_event_name(event: mpv_event_id) -> *const c_char {
    pfn_mpv_event_name.unwrap()(event)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_wait_event(ctx: *mut mpv_handle, timeout: f64) -> *mut mpv_event {
    pfn_mpv_wait_event.unwrap()(ctx, timeout)
}
#[cfg(target_os = "windows")]
pub unsafe fn mpv_wakeup(ctx: *mut mpv_handle) {
    pfn_mpv_wakeup.unwrap()(ctx)
}
