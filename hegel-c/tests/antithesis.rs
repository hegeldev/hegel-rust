//! The Antithesis integration driven through the C ABI with
//! `ANTITHESIS_OUTPUT_DIR` really set: every run and blob replay whose
//! settings carry a test location writes its verdict to `sdk.jsonl`.
//!
//! The variable is process-wide, so it lives in its own test binary, is set
//! once, and the tests take turns (each truncating the file first).

#![cfg(not(windows))]

mod common;

use common::{make_settings, next_case, ok, start};
use hegel_c::hegel_result_t::HEGEL_OK;
use hegel_c::{
    HegelContext, HegelFailure, HegelRun, HegelRunResult, HegelSettings, HegelTestCase,
    hegel_context_free, hegel_context_new, hegel_failure_free, hegel_failure_reproduction_blob,
    hegel_generate_integer, hegel_mark_complete, hegel_run_free, hegel_run_result,
    hegel_run_result_failure, hegel_run_result_status, hegel_run_status_t, hegel_settings_free,
    hegel_settings_new_for_profile, hegel_settings_set_database, hegel_settings_set_test_location,
    hegel_status_t, hegel_test_case_free, hegel_test_case_from_blob,
};
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

static OUTPUT_DIR: OnceLock<TempDir> = OnceLock::new();
static TURN: Mutex<()> = Mutex::new(());

/// Take the turn: point `ANTITHESIS_OUTPUT_DIR` at the shared directory
/// (once) and remove any `sdk.jsonl` a previous test left there.
fn take_turn() -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
    let guard = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = OUTPUT_DIR.get_or_init(|| {
        let dir = TempDir::new().unwrap();
        unsafe { std::env::set_var("ANTITHESIS_OUTPUT_DIR", dir.path()) };
        dir
    });
    let sdk = dir.path().join("sdk.jsonl");
    if sdk.exists() {
        std::fs::remove_file(&sdk).unwrap();
    }
    (guard, sdk)
}

const FILE: &CStr = c"tests/antithesis.rs";
const CLASS: &CStr = c"antithesis";
const FUNCTION: &CStr = c"fixture";

unsafe fn set_location(ctx: *mut HegelContext, s: *mut HegelSettings) {
    ok(unsafe {
        hegel_settings_set_test_location(
            ctx,
            s,
            FILE.as_ptr(),
            7,
            CLASS.as_ptr(),
            FUNCTION.as_ptr(),
        )
    });
}

fn expected_event(hit: bool, condition: bool) -> serde_json::Value {
    let id = "antithesis::fixture passes properties";
    serde_json::json!({
        "antithesis_assert": {
            "hit": hit,
            "must_hit": true,
            "assert_type": "always",
            "display_type": "Always",
            "condition": condition,
            "id": id,
            "message": id,
            "location": {
                "class": "antithesis",
                "function": "fixture",
                "file": "tests/antithesis.rs",
                "begin_line": 7,
                "begin_column": 0,
            },
        }
    })
}

fn read_events(sdk: &PathBuf) -> Vec<serde_json::Value> {
    std::fs::read_to_string(sdk)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Drive a run to completion, drawing one integer per case and completing
/// each with `status` (or as an overrun when the draw is refused, as it is
/// while the shrinker probes shorter choice sequences), and return its
/// result.
unsafe fn drive(
    ctx: *mut HegelContext,
    run: *mut HegelRun,
    status: hegel_status_t,
) -> *mut HegelRunResult {
    unsafe {
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut value = 0i64;
            let status = if hegel_generate_integer(ctx, tc, 0, 100, &mut value) == HEGEL_OK {
                status
            } else {
                hegel_status_t::HEGEL_STATUS_OVERRUN
            };
            ok(hegel_mark_complete(
                ctx,
                tc,
                status as u32,
                c"the bug".as_ptr(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        let mut r: *mut HegelRunResult = ptr::null_mut();
        ok(hegel_run_result(ctx, run, &mut r));
        r
    }
}

unsafe fn status_of(ctx: *mut HegelContext, r: *const HegelRunResult) -> hegel_run_status_t {
    let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
    ok(unsafe { hegel_run_result_status(ctx, r, &mut status) });
    status
}

#[test]
fn a_passing_run_reports_a_true_condition() {
    let (_turn, sdk) = take_turn();
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        set_location(ctx, s);
        let run = start(ctx, s);
        let r = drive(ctx, run, hegel_status_t::HEGEL_STATUS_VALID);
        assert!(status_of(ctx, r) == hegel_run_status_t::HEGEL_RUN_STATUS_PASSED);
        assert_eq!(
            read_events(&sdk),
            [expected_event(false, false), expected_event(true, true)]
        );
        ok(hegel_c::hegel_run_result_free(ctx, r));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn a_failing_run_reports_a_false_condition_and_its_blob_replays_do_too() {
    let (_turn, sdk) = take_turn();
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        set_location(ctx, s);
        let run = start(ctx, s);
        let r = drive(ctx, run, hegel_status_t::HEGEL_STATUS_INTERESTING);
        assert!(status_of(ctx, r) == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        assert_eq!(
            read_events(&sdk),
            [expected_event(false, false), expected_event(true, false)]
        );

        let mut f: *mut HegelFailure = ptr::null_mut();
        ok(hegel_run_result_failure(ctx, r, 0, &mut f));
        let mut blob_ptr: *const std::ffi::c_char = ptr::null();
        ok(hegel_failure_reproduction_blob(ctx, f, &mut blob_ptr));
        let blob: CString = CStr::from_ptr(blob_ptr).to_owned();
        ok(hegel_failure_free(ctx, f));
        ok(hegel_c::hegel_run_result_free(ctx, r));
        ok(hegel_run_free(ctx, run));

        for (status, passed) in [
            (hegel_status_t::HEGEL_STATUS_INTERESTING, false),
            (hegel_status_t::HEGEL_STATUS_VALID, true),
        ] {
            std::fs::remove_file(&sdk).unwrap();
            let mut tc: *mut HegelTestCase = ptr::null_mut();
            ok(hegel_test_case_from_blob(
                ctx,
                s,
                blob.as_ptr(),
                None,
                ptr::null_mut(),
                &mut tc,
            ));
            let mut value = 0i64;
            ok(hegel_generate_integer(ctx, tc, 0, 100, &mut value));
            assert!(!sdk.exists(), "nothing is reported before completion");
            ok(hegel_mark_complete(
                ctx,
                tc,
                status as u32,
                c"the bug".as_ptr(),
            ));
            assert_eq!(
                read_events(&sdk),
                [expected_event(false, false), expected_event(true, passed)]
            );
            ok(hegel_test_case_free(ctx, tc));
        }

        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn a_run_without_a_test_location_reports_nothing() {
    let (_turn, sdk) = take_turn();
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let run = start(ctx, s);
        let r = drive(ctx, run, hegel_status_t::HEGEL_STATUS_VALID);
        assert!(status_of(ctx, r) == hegel_run_status_t::HEGEL_RUN_STATUS_PASSED);
        assert!(!sdk.exists());
        ok(hegel_c::hegel_run_result_free(ctx, r));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

/// A run-level error is no verdict on the property, so it is reported as a
/// failure. The `workload` profile Antithesis detection selects suppresses
/// every health check, so the run resolves the plain `base` settings and
/// rejects every test case, failing the `FilterTooMuch` check.
#[test]
fn a_run_that_errors_reports_a_false_condition() {
    let (_turn, sdk) = take_turn();
    let ctx = hegel_context_new();
    unsafe {
        let mut s: *mut HegelSettings = ptr::null_mut();
        ok(hegel_settings_new_for_profile(
            ctx,
            c"base".as_ptr(),
            &mut s,
        ));
        ok(hegel_settings_set_database(ctx, s, c"".as_ptr()));
        set_location(ctx, s);
        let run = start(ctx, s);
        let r = drive(ctx, run, hegel_status_t::HEGEL_STATUS_INVALID);
        assert!(status_of(ctx, r) == hegel_run_status_t::HEGEL_RUN_STATUS_ERROR);
        assert_eq!(
            read_events(&sdk),
            [expected_event(false, false), expected_event(true, false)]
        );
        ok(hegel_c::hegel_run_result_free(ctx, r));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}
