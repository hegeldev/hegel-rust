// End-to-end test of libhegel via dlopen.
//
// We load the built libhegel.so as if we were a non-Rust caller. This
// catches FFI-layer issues (symbol visibility, layout, channel deadlocks)
// that a Rust-side unit test would miss.

use std::ffi::{CStr, CString, c_char, c_int};
use std::path::PathBuf;
use std::ptr;

use ciborium::Value;
use libloading::{Library, Symbol};

// ─── Library loading ────────────────────────────────────────────────────────

fn lib_path() -> PathBuf {
    // The crate is part of a workspace, so the cdylib lands in
    // ../target/{debug,release}/libhegel.<ext>. `cargo test` builds the debug
    // profile by default; for --release tests we look there too.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.parent().unwrap().join("target");
    let filename = if cfg!(target_os = "macos") {
        "libhegel.dylib"
    } else if cfg!(target_os = "windows") {
        "hegel.dll"
    } else {
        "libhegel.so"
    };
    for profile in ["debug", "release"] {
        let candidate = target_dir.join(profile).join(filename);
        if candidate.exists() {
            return candidate;
        }
    }
    panic!(
        "could not find {} under {}; run `cargo build -p hegeltest-c` first",
        filename,
        target_dir.display()
    );
}

unsafe fn load() -> Library {
    unsafe { Library::new(lib_path()).expect("dlopen libhegel") }
}

// ─── Symbol typedefs ────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum CStatus {
    Valid = 0,
    Invalid = 1,
    Overrun = 2,
    Interesting = 3,
}

type FnSettingsNew = unsafe extern "C" fn() -> *mut u8;
type FnSettingsFree = unsafe extern "C" fn(*mut u8);
type FnSettingsTestCases = unsafe extern "C" fn(*mut u8, u64);
type FnSettingsDatabase = unsafe extern "C" fn(*mut u8, *const c_char);
type FnSettingsDatabaseKey = unsafe extern "C" fn(*mut u8, *const c_char);
type FnSettingsSeed = unsafe extern "C" fn(*mut u8, u64, bool);
type FnSettingsDerandomize = unsafe extern "C" fn(*mut u8, bool);
type FnRunStart = unsafe extern "C" fn(*const u8) -> *mut u8;
type FnNextTestCase = unsafe extern "C" fn(*mut u8) -> *mut u8;
type FnRunResult = unsafe extern "C" fn(*mut u8) -> *const u8;
type FnRunFree = unsafe extern "C" fn(*mut u8);
type FnGenerate =
    unsafe extern "C" fn(*mut u8, *const u8, usize, *mut *const u8, *mut usize) -> c_int;
type FnMarkComplete = unsafe extern "C" fn(*mut u8, CStatus, *const c_char) -> c_int;
type FnRunResultPassed = unsafe extern "C" fn(*const u8) -> bool;
type FnRunResultFailureCount = unsafe extern "C" fn(*const u8) -> usize;
type FnRunResultFailure = unsafe extern "C" fn(*const u8, usize) -> *const u8;
type FnFailureOrigin = unsafe extern "C" fn(*const u8) -> *const c_char;
type FnFailurePanicMessage = unsafe extern "C" fn(*const u8) -> *const c_char;
type FnLastErrorMessage = unsafe extern "C" fn() -> *const c_char;

// Bundle of the symbols we use, so the test bodies stay readable.
struct Api<'a> {
    settings_new: Symbol<'a, FnSettingsNew>,
    settings_free: Symbol<'a, FnSettingsFree>,
    settings_test_cases: Symbol<'a, FnSettingsTestCases>,
    settings_database: Symbol<'a, FnSettingsDatabase>,
    settings_database_key: Symbol<'a, FnSettingsDatabaseKey>,
    settings_seed: Symbol<'a, FnSettingsSeed>,
    settings_derandomize: Symbol<'a, FnSettingsDerandomize>,
    run_start: Symbol<'a, FnRunStart>,
    next_test_case: Symbol<'a, FnNextTestCase>,
    run_result: Symbol<'a, FnRunResult>,
    run_free: Symbol<'a, FnRunFree>,
    generate: Symbol<'a, FnGenerate>,
    mark_complete: Symbol<'a, FnMarkComplete>,
    run_result_passed: Symbol<'a, FnRunResultPassed>,
    run_result_failure_count: Symbol<'a, FnRunResultFailureCount>,
    run_result_failure: Symbol<'a, FnRunResultFailure>,
    failure_origin: Symbol<'a, FnFailureOrigin>,
    failure_panic_message: Symbol<'a, FnFailurePanicMessage>,
    last_error_message: Symbol<'a, FnLastErrorMessage>,
}

unsafe fn bind(lib: &Library) -> Api<'_> {
    unsafe {
        Api {
            settings_new: lib.get(b"hegel_settings_new\0").unwrap(),
            settings_free: lib.get(b"hegel_settings_free\0").unwrap(),
            settings_test_cases: lib.get(b"hegel_settings_test_cases\0").unwrap(),
            settings_database: lib.get(b"hegel_settings_database\0").unwrap(),
            settings_database_key: lib.get(b"hegel_settings_database_key\0").unwrap(),
            settings_seed: lib.get(b"hegel_settings_seed\0").unwrap(),
            settings_derandomize: lib.get(b"hegel_settings_derandomize\0").unwrap(),
            run_start: lib.get(b"hegel_run_start\0").unwrap(),
            next_test_case: lib.get(b"hegel_next_test_case\0").unwrap(),
            run_result: lib.get(b"hegel_run_result\0").unwrap(),
            run_free: lib.get(b"hegel_run_free\0").unwrap(),
            generate: lib.get(b"hegel_generate\0").unwrap(),
            mark_complete: lib.get(b"hegel_mark_complete\0").unwrap(),
            run_result_passed: lib.get(b"hegel_run_result_passed\0").unwrap(),
            run_result_failure_count: lib.get(b"hegel_run_result_failure_count\0").unwrap(),
            run_result_failure: lib.get(b"hegel_run_result_failure\0").unwrap(),
            failure_origin: lib.get(b"hegel_failure_origin\0").unwrap(),
            failure_panic_message: lib.get(b"hegel_failure_panic_message\0").unwrap(),
            last_error_message: lib.get(b"hegel_last_error_message\0").unwrap(),
        }
    }
}

// ─── CBOR schema helpers ────────────────────────────────────────────────────

fn encode(value: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).unwrap();
    buf
}

fn decode(bytes: &[u8]) -> Value {
    ciborium::de::from_reader(bytes).unwrap()
}

fn integer_schema(min: i64, max: i64) -> Vec<u8> {
    let mut entries: Vec<(Value, Value)> = Vec::new();
    entries.push((Value::Text("type".into()), Value::Text("integer".into())));
    entries.push((Value::Text("min_value".into()), Value::Integer(min.into())));
    entries.push((Value::Text("max_value".into()), Value::Integer(max.into())));
    encode(&Value::Map(entries))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn libhegel_runs_passing_property() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();
        assert!(!s.is_null());
        (a.settings_test_cases)(s, 25);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 1, true);

        let run = (a.run_start)(s);
        assert!(!run.is_null());

        let schema = integer_schema(0, 100);
        let mut cases = 0usize;
        loop {
            let tc = (a.next_test_case)(run);
            if tc.is_null() {
                let err = CStr::from_ptr((a.last_error_message)()).to_string_lossy();
                assert_eq!(err, "", "next_test_case returned NULL with error: {}", err);
                break;
            }
            cases += 1;

            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            assert_eq!(rc, 0, "generate failed: rc={}", rc);

            let val_bytes = std::slice::from_raw_parts(val_ptr, val_len);
            let v = decode(val_bytes);
            // Sanity: value is in [0, 100].
            if let Value::Integer(i) = v {
                let n: i128 = i.into();
                assert!((0..=100).contains(&n), "got out-of-range value {}", n);
            } else {
                panic!("expected integer, got {:?}", v);
            }

            let mc = (a.mark_complete)(tc, CStatus::Valid, ptr::null());
            assert_eq!(mc, 0);
        }
        assert!(cases >= 1, "expected at least one test case to run");

        let result = (a.run_result)(run);
        assert!(!result.is_null(), "run_result null after drained loop");
        assert!((a.run_result_passed)(result), "expected passing run");
        assert_eq!((a.run_result_failure_count)(result), 0);

        (a.run_free)(run);
        (a.settings_free)(s);
    }
}

#[test]
fn libhegel_reports_shrunk_failure() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 200);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 0xc0ffee, true);

        let run = (a.run_start)(s);
        let schema = integer_schema(0, 100);
        let origin = CString::new("n >= 5 failed").unwrap();

        loop {
            let tc = (a.next_test_case)(run);
            if tc.is_null() {
                let err = CStr::from_ptr((a.last_error_message)()).to_string_lossy();
                assert_eq!(err, "", "got error mid-loop: {}", err);
                break;
            }

            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            if rc == -1 {
                // HEGEL_E_STOP_TEST — engine exhausted during a shrink probe.
                (a.mark_complete)(tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, 0, "unexpected generate rc={}", rc);

            let v = decode(std::slice::from_raw_parts(val_ptr, val_len));
            let Value::Integer(i) = v else {
                panic!("expected int")
            };
            let n: i128 = i.into();

            let status = if n < 5 {
                CStatus::Valid
            } else {
                CStatus::Interesting
            };
            let origin_ptr = if matches!(status, CStatus::Interesting) {
                origin.as_ptr()
            } else {
                ptr::null()
            };
            (a.mark_complete)(tc, status, origin_ptr);
        }

        let result = (a.run_result)(run);
        assert!(!result.is_null());
        assert!(
            !(a.run_result_passed)(result),
            "expected failing run (predicate n < 5 is false for many n in [0,100])"
        );
        let n_failures = (a.run_result_failure_count)(result);
        assert!(n_failures >= 1, "expected at least one failure");

        // Inspect the first failure: origin should be the string we passed in.
        let f = (a.run_result_failure)(result, 0);
        assert!(!f.is_null());
        let origin_back = CStr::from_ptr((a.failure_origin)(f)).to_string_lossy();
        assert!(
            origin_back.contains("n >= 5 failed"),
            "expected failure origin to contain 'n >= 5 failed', got: {}",
            origin_back
        );

        (a.run_free)(run);
        (a.settings_free)(s);
    }
}

#[test]
fn next_test_case_without_mark_complete_errors() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 5);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 7, true);

        let run = (a.run_start)(s);
        let tc1 = (a.next_test_case)(run);
        assert!(!tc1.is_null());
        // Deliberately skip mark_complete.
        let tc2 = (a.next_test_case)(run);
        assert!(tc2.is_null(), "expected NULL on second next_test_case");
        let err = CStr::from_ptr((a.last_error_message)())
            .to_string_lossy()
            .into_owned();
        assert!(err.contains("not marked complete"), "got: {}", err);

        // Now mark first complete and let the loop drain so run_free is clean.
        (a.mark_complete)(tc1, CStatus::Valid, ptr::null());
        loop {
            let tc = (a.next_test_case)(run);
            if tc.is_null() {
                break;
            }
            (a.mark_complete)(tc, CStatus::Valid, ptr::null());
        }

        (a.run_free)(run);
        (a.settings_free)(s);
    }
}

#[test]
fn run_free_after_early_exit_does_not_hang() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 100);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 99, true);

        let run = (a.run_start)(s);
        // Grab one test case, don't complete it, jump straight to free.
        let _ = (a.next_test_case)(run);
        (a.run_free)(run);
        (a.settings_free)(s);
        // Reaching here without deadlocking is the assertion.
    }
}

/// Reproduces hegel-go report #2 via the C API: persist a failing example
/// on run 1, then run 2 with the same database + key and confirm the
/// first test case is a replay of the persisted (shrunk) failing value.
///
/// If this test passes but hegel-go still sees the bug, the issue is in
/// hegel-go's database / key plumbing rather than in libhegel.
#[test]
fn libhegel_replays_persisted_failure_with_same_database_key() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    let tempdir = tempfile::TempDir::new().expect("tempdir");
    let db_path = CString::new(tempdir.path().to_string_lossy().as_bytes()).unwrap();
    let key = CString::new("replay-smoke").unwrap();
    let schema = integer_schema(0, 2_000_000);

    let predicate = |n: i128| n >= 1_000_000;

    // ---- run 1 ----
    let mut last_failure: Option<i128> = None;
    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 200);
        (a.settings_database)(s, db_path.as_ptr());
        (a.settings_database_key)(s, key.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 1, true);

        let run = (a.run_start)(s);
        loop {
            let tc = (a.next_test_case)(run);
            if tc.is_null() {
                break;
            }
            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            if rc == -1 {
                (a.mark_complete)(tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, 0);
            let v = decode(std::slice::from_raw_parts(val_ptr, val_len));
            let Value::Integer(i) = v else {
                panic!("expected integer")
            };
            let n: i128 = i.into();
            if predicate(n) {
                last_failure = Some(n);
                let origin = CString::new("n >= 1_000_000").unwrap();
                (a.mark_complete)(tc, CStatus::Interesting, origin.as_ptr());
            } else {
                (a.mark_complete)(tc, CStatus::Valid, ptr::null());
            }
        }
        (a.run_free)(run);
        (a.settings_free)(s);
    }
    assert!(last_failure.is_some(), "run 1 never observed the failure");

    // ---- run 2: same key + same db, expect replay first ----
    let mut first_seen: Option<i128> = None;
    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 200);
        (a.settings_database)(s, db_path.as_ptr());
        (a.settings_database_key)(s, key.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 1, true);

        let run = (a.run_start)(s);
        loop {
            let tc = (a.next_test_case)(run);
            if tc.is_null() {
                break;
            }
            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            if rc == -1 {
                (a.mark_complete)(tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, 0);
            let v = decode(std::slice::from_raw_parts(val_ptr, val_len));
            let Value::Integer(i) = v else {
                panic!("expected integer")
            };
            let n: i128 = i.into();
            if first_seen.is_none() {
                first_seen = Some(n);
            }
            if predicate(n) {
                let origin = CString::new("n >= 1_000_000").unwrap();
                (a.mark_complete)(tc, CStatus::Interesting, origin.as_ptr());
            } else {
                (a.mark_complete)(tc, CStatus::Valid, ptr::null());
            }
        }
        (a.run_free)(run);
        (a.settings_free)(s);
    }

    let first = first_seen.expect("run 2 never received a test case");
    assert!(
        predicate(first),
        "expected replay of n>=1_000_000 as first test case, got n={}",
        first
    );
}

#[test]
fn engine_panic_surfaces_as_failure_not_worker_crash() {
    // Reproduces hegel-go report #1: a property whose draws are all
    // rejected via `assume` triggers `FilterTooMuch` inside `run_main`,
    // which `panic!`s the worker thread. Before catch_unwind was added
    // around `run_native`, the worker died and the C caller saw a generic
    // "worker terminated" error. After the fix, the panic message is
    // wrapped in a `HegelFailure` and returned via `hegel_run_result`.
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 200);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 1, true);

        let run = (a.run_start)(s);
        let schema = integer_schema(0, 1_000_000);

        // Reject everything we draw. The engine eventually trips
        // FilterTooMuch and panics.
        loop {
            let tc = (a.next_test_case)(run);
            if tc.is_null() {
                break;
            }
            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            let _ = (val_ptr, val_len);
            if rc == -1 {
                (a.mark_complete)(tc, CStatus::Overrun, ptr::null());
            } else {
                (a.mark_complete)(tc, CStatus::Invalid, ptr::null());
            }
        }

        let last_err = CStr::from_ptr((a.last_error_message)())
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            last_err, "",
            "next_test_case loop ended with error instead of normal completion: {}",
            last_err
        );

        let result = (a.run_result)(run);
        assert!(
            !result.is_null(),
            "hegel_run_result returned NULL after engine panic; \
             last_error = {}",
            CStr::from_ptr((a.last_error_message)()).to_string_lossy()
        );

        // The run must be marked failing, with the FilterTooMuch panic
        // text reachable through the failure API.
        assert!(
            !(a.run_result_passed)(result),
            "expected failing run after engine panic"
        );
        assert!(
            (a.run_result_failure_count)(result) >= 1,
            "expected at least one failure for the engine panic"
        );
        let f = (a.run_result_failure)(result, 0);
        assert!(!f.is_null());
        let msg = CStr::from_ptr((a.failure_panic_message)(f)).to_string_lossy();
        assert!(
            msg.contains("FilterTooMuch") || msg.contains("FailedHealthCheck"),
            "expected panic message to reference FilterTooMuch / FailedHealthCheck, got: {}",
            msg
        );

        (a.run_free)(run);
        (a.settings_free)(s);
    }
}
