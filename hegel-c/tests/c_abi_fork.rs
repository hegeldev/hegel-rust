//! Pins libhegel's behavior under `fork(2)`.
//!
//! The engine runs entirely on the calling thread and holds no locks or
//! file descriptors between C-ABI calls, so a single-threaded client may
//! fork between them. One test drives fork-per-test-case with every engine
//! call in the parent, the pattern a C harness uses for crash isolation.
//! The other has the child call into its copy of the engine and checks
//! that the parent's run is undisturbed.

#![cfg(unix)]

mod common;

use common::{make_settings, make_settings_no_db, next_case, ok, start};
use hegel_c::hegel_result_t::*;
use hegel_c::{
    hegel_context_free, hegel_context_new, hegel_failure_free, hegel_failure_reproduction_blob,
    hegel_generate_boolean, hegel_generate_integer, hegel_mark_complete, hegel_run_free,
    hegel_run_result, hegel_run_result_failure, hegel_run_result_failure_count,
    hegel_run_result_free, hegel_run_result_status, hegel_run_status_t, hegel_settings_free,
    hegel_settings_set_database, hegel_settings_set_database_key, hegel_status_t,
    hegel_test_case_free, hegel_test_case_from_blob,
};
use std::ffi::CString;
use std::ptr;

unsafe extern "C" {
    fn fork() -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn _exit(code: i32) -> !;
}

fn wait_for_exit_code(pid: i32) -> i32 {
    let mut status = 0;
    loop {
        let r = unsafe { waitpid(pid, &mut status, 0) };
        if r == pid {
            break;
        }
        assert_eq!(r, -1);
    }
    assert_eq!(status & 0x7f, 0, "child did not exit normally: {status}");
    (status >> 8) & 0xff
}

#[test]
fn fork_per_case_with_parent_driving_the_engine_finds_and_shrinks_failures() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings(ctx);
        let db = CString::new(dir.path().to_str().unwrap()).unwrap();
        ok(hegel_settings_set_database(ctx, s, db.as_ptr()));
        ok(hegel_settings_set_database_key(
            ctx,
            s,
            c"fork-test".as_ptr(),
        ));
        let run = start(ctx, s);
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut n = 0i64;
            let rc = hegel_generate_integer(ctx, tc, 0, 1000, &mut n);
            if rc == HEGEL_E_STOP_TEST {
                ok(hegel_mark_complete(
                    ctx,
                    tc,
                    hegel_status_t::HEGEL_STATUS_OVERRUN as u32,
                    ptr::null(),
                ));
                ok(hegel_test_case_free(ctx, tc));
                continue;
            }
            ok(rc);
            let pid = fork();
            assert!(pid >= 0);
            if pid == 0 {
                _exit(if n >= 100 { 1 } else { 0 });
            }
            let status = if wait_for_exit_code(pid) == 0 {
                hegel_status_t::HEGEL_STATUS_VALID
            } else {
                hegel_status_t::HEGEL_STATUS_INTERESTING
            };
            ok(hegel_mark_complete(
                ctx,
                tc,
                status as u32,
                c"n >= 100".as_ptr(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }

        let mut res = ptr::null_mut();
        ok(hegel_run_result(ctx, run, &mut res));
        let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_PASSED;
        ok(hegel_run_result_status(ctx, res, &mut status));
        assert!(status == hegel_run_status_t::HEGEL_RUN_STATUS_FAILED);
        let mut count = 0usize;
        ok(hegel_run_result_failure_count(ctx, res, &mut count));
        assert_eq!(count, 1);

        let mut f = ptr::null_mut();
        ok(hegel_run_result_failure(ctx, res, 0, &mut f));
        let mut blob = ptr::null();
        ok(hegel_failure_reproduction_blob(ctx, f, &mut blob));
        assert!(!blob.is_null());

        let mut replay = ptr::null_mut();
        ok(hegel_test_case_from_blob(
            ctx,
            s,
            blob,
            None,
            ptr::null_mut(),
            &mut replay,
        ));
        let mut shrunk = 0i64;
        ok(hegel_generate_integer(ctx, replay, 0, 1000, &mut shrunk));
        assert_eq!(shrunk, 100);
        ok(hegel_mark_complete(
            ctx,
            replay,
            hegel_status_t::HEGEL_STATUS_INTERESTING as u32,
            c"n >= 100".as_ptr(),
        ));
        ok(hegel_test_case_free(ctx, replay));

        assert!(std::fs::read_dir(dir.path()).unwrap().count() > 0);

        ok(hegel_failure_free(ctx, f));
        ok(hegel_run_result_free(ctx, res));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn engine_calls_in_a_forked_child_leave_the_parent_run_intact() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings_no_db(ctx);
        let run = start(ctx, s);
        let mut cases = 0u64;
        loop {
            let tc = next_case(ctx, run);
            if tc.is_null() {
                break;
            }
            cases += 1;
            let pid = fork();
            assert!(pid >= 0);
            if pid == 0 {
                let mut b = false;
                let drew = hegel_generate_boolean(ctx, tc, 0.5, false, false, &mut b);
                let marked = hegel_mark_complete(
                    ctx,
                    tc,
                    hegel_status_t::HEGEL_STATUS_VALID as u32,
                    ptr::null(),
                );
                let freed = hegel_test_case_free(ctx, tc);
                _exit(
                    if drew == HEGEL_OK && marked == HEGEL_OK && freed == HEGEL_OK {
                        0
                    } else {
                        1
                    },
                );
            }
            assert_eq!(wait_for_exit_code(pid), 0);

            let mut b = false;
            ok(hegel_generate_boolean(ctx, tc, 0.5, false, false, &mut b));
            ok(hegel_mark_complete(
                ctx,
                tc,
                hegel_status_t::HEGEL_STATUS_VALID as u32,
                ptr::null(),
            ));
            ok(hegel_test_case_free(ctx, tc));
        }
        assert!(cases > 0);

        let mut res = ptr::null_mut();
        ok(hegel_run_result(ctx, run, &mut res));
        let mut status = hegel_run_status_t::HEGEL_RUN_STATUS_FAILED;
        ok(hegel_run_result_status(ctx, res, &mut status));
        assert!(status == hegel_run_status_t::HEGEL_RUN_STATUS_PASSED);

        ok(hegel_run_result_free(ctx, res));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}
