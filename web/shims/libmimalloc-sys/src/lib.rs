//! The calls `horizon-threads` makes into mimalloc, as no-ops for the browser
//! build: there is no OS to give pages back to and no allocator statistics.
#![allow(
    non_camel_case_types,
    clippy::missing_safety_doc,
    clippy::too_many_arguments
)]

use std::ffi::{c_char, c_long, c_void};

pub type mi_option_t = i32;

pub unsafe fn mi_option_set(_option: mi_option_t, _value: c_long) {}
pub unsafe fn mi_option_get(_option: mi_option_t) -> c_long {
    0
}
pub unsafe fn mi_collect(_force: bool) {}
pub unsafe fn mi_process_info(
    _elapsed: *mut usize,
    _user: *mut usize,
    _system: *mut usize,
    _rss: *mut usize,
    _peak_rss: *mut usize,
    _commit: *mut usize,
    _peak_commit: *mut usize,
    _faults: *mut usize,
) {
}
pub unsafe fn mi_stats_get_json(_size: usize, _buf: *mut c_char) -> *mut c_char {
    std::ptr::null_mut()
}
pub unsafe fn mi_free(_p: *mut c_void) {}
