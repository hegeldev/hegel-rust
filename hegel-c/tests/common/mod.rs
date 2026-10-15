//! Shared helpers for the in-process C-ABI test binaries.

use hegel_c::hegel_result_t::*;
use hegel_c::{
    HegelContext, HegelRun, HegelSettings, HegelTestCase, hegel_context_last_error,
    hegel_next_test_case, hegel_output_callback_t, hegel_run_start, hegel_settings_new,
    hegel_settings_set_database,
};
use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;

/// Assert a call that should always succeed in these tests returned `HEGEL_OK`.
#[allow(dead_code)]
pub fn ok(rc: hegel_c::hegel_result_t) {
    assert_eq!(rc, HEGEL_OK);
}

#[allow(dead_code)]
pub fn last_error(ctx: *const HegelContext) -> String {
    let p = unsafe { hegel_context_last_error(ctx) };
    if p.data.is_null() {
        String::new()
    } else {
        String::from_utf8_lossy(unsafe { test_bytes(p) }).into_owned()
    }
}

#[allow(dead_code)]
pub unsafe fn make_settings(ctx: *mut HegelContext) -> *mut HegelSettings {
    let mut s: *mut HegelSettings = ptr::null_mut();
    assert_eq!(unsafe { hegel_settings_new(ctx, &mut s) }, HEGEL_OK);
    assert!(!s.is_null());
    s
}

/// Like [`make_settings`], but with the failure database disabled.
#[allow(dead_code)]
pub unsafe fn make_settings_no_db(ctx: *mut HegelContext) -> *mut HegelSettings {
    let s = unsafe { make_settings(ctx) };
    let empty = CString::new("").unwrap();
    assert_eq!(
        unsafe { hegel_settings_set_database(ctx, s, test_optional(&test_view(empty.as_ptr()))) },
        HEGEL_OK
    );
    s
}

#[allow(dead_code)]
pub unsafe fn start(ctx: *mut HegelContext, s: *const HegelSettings) -> *mut HegelRun {
    unsafe { start_with_output(ctx, s, None, ptr::null_mut()) }
}

/// Like [`start`], but routing the run's engine output to `callback` /
/// `user_data` (see `hegel_run_start`).
#[allow(dead_code)]
pub unsafe fn start_with_output(
    ctx: *mut HegelContext,
    s: *const HegelSettings,
    callback: hegel_output_callback_t,
    user_data: *mut c_void,
) -> *mut HegelRun {
    let mut run: *mut HegelRun = ptr::null_mut();
    assert_eq!(
        unsafe { hegel_run_start(ctx, s, callback, user_data, &mut run) },
        HEGEL_OK
    );
    assert!(!run.is_null());
    run
}

/// The next case the run hands out, or null at completion (`HEGEL_OK` + null).
#[allow(dead_code)]
pub unsafe fn next_case(ctx: *mut HegelContext, run: *mut HegelRun) -> *mut HegelTestCase {
    let mut tc: *mut HegelTestCase = ptr::null_mut();
    assert_eq!(unsafe { hegel_next_test_case(ctx, run, &mut tc) }, HEGEL_OK);
    tc
}

unsafe fn test_view(data: *const std::ffi::c_char) -> hegel_c::hegel_string_t {
    hegel_c::hegel_string_t {
        data,
        len: if data.is_null() {
            0
        } else {
            unsafe { std::ffi::CStr::from_ptr(data) }.to_bytes().len()
        },
    }
}

unsafe fn test_bytes(value: hegel_c::hegel_string_t) -> &'static [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(value.data.cast(), value.len) }
    }
}

fn test_optional(value: &hegel_c::hegel_string_t) -> *const hegel_c::hegel_string_t {
    if value.data.is_null() && value.len == 0 {
        std::ptr::null()
    } else {
        value
    }
}
