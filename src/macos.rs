//! Narrow wrappers for public macOS APIs. No focus changes or editor text reads.
use anyhow::{Result, bail};
use std::{
    ffi::{CStr, c_char, c_void},
    ptr::NonNull,
};

unsafe extern "C" {
    fn ptt_mac_capture_target() -> *mut c_void;
    fn ptt_mac_release_target(target: *mut c_void);
    fn ptt_mac_tap_enter(target: *mut c_void) -> i32;
    fn ptt_mac_network_rank(index: u32) -> u8;
    fn ptt_mac_installation() -> i32;
    fn ptt_mac_doubao_selected() -> bool;
    fn ptt_mac_decode_settings(bytes: *const u8, length: usize) -> *mut c_char;
    fn ptt_mac_free_string(value: *mut c_char);
}

pub(crate) struct EnterTarget(NonNull<c_void>);
impl Drop for EnterTarget {
    fn drop(&mut self) {
        unsafe {
            ptt_mac_release_target(self.0.as_ptr());
        }
    }
}
pub(crate) fn capture_enter_target() -> Option<EnterTarget> {
    NonNull::new(unsafe { ptt_mac_capture_target() }).map(EnterTarget)
}
pub(crate) fn tap_enter(target: Option<EnterTarget>) -> Result<Option<&'static str>> {
    let Some(target) = target else {
        return Ok(Some("target_unavailable"));
    };
    match unsafe { ptt_mac_tap_enter(target.0.as_ptr()) } {
        0 => Ok(None),
        1 => Ok(Some("focus_changed")),
        2 => Ok(Some("key_held")),
        3 => Ok(Some("target_unavailable")),
        4 => Ok(Some("permission_denied")),
        _ => bail!("macOS 无法创建 Enter 按键事件；本次不重试"),
    }
}
pub(crate) fn network_rank(index: u32) -> u8 {
    unsafe { ptt_mac_network_rank(index) }
}
pub(crate) fn installation() -> i32 {
    unsafe { ptt_mac_installation() }
}
pub(crate) fn doubao_selected() -> bool {
    unsafe { ptt_mac_doubao_selected() }
}
pub(crate) fn decode_settings(bytes: &[u8]) -> Result<serde_json::Value> {
    let pointer = unsafe { ptt_mac_decode_settings(bytes.as_ptr(), bytes.len()) };
    anyhow::ensure!(!pointer.is_null(), "无法识别豆包设置归档，未读取其他数据");
    let result = serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes());
    unsafe {
        ptt_mac_free_string(pointer);
    }
    Ok(result?)
}
