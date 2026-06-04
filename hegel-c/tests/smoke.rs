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
    let filename = if cfg!(target_os = "macos") {
        "libhegel.dylib"
    } else if cfg!(target_os = "windows") {
        "hegel.dll"
    } else {
        "libhegel.so"
    };
    // `HEGEL_C_LIB_DIR` lets the harness load a library built into a separate
    // target dir — e.g. the `panic = "abort"` build produced by
    // `just c-test-abort`, which proves no panic crosses the FFI boundary.
    if let Ok(dir) = std::env::var("HEGEL_C_LIB_DIR") {
        let candidate = PathBuf::from(dir).join(filename);
        assert!(
            candidate.exists(),
            "HEGEL_C_LIB_DIR is set but {} does not exist",
            candidate.display()
        );
        return candidate;
    }
    // The crate is part of a workspace, so the cdylib lands in
    // ../target/{debug,release}/libhegel.<ext>. `cargo test` builds the debug
    // profile by default; for --release tests we look there too.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.parent().unwrap().join("target");
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
type FnNewPool = unsafe extern "C" fn(*mut u8, *mut i64) -> c_int;
type FnPoolAdd = unsafe extern "C" fn(*mut u8, i64, *mut i64) -> c_int;
type FnPoolGenerate = unsafe extern "C" fn(*mut u8, i64, bool, *mut i64) -> c_int;
type FnRunResultPassed = unsafe extern "C" fn(*const u8) -> bool;
type FnRunResultFailureCount = unsafe extern "C" fn(*const u8) -> usize;
type FnRunResultFailure = unsafe extern "C" fn(*const u8, usize) -> *const u8;
type FnFailureOrigin = unsafe extern "C" fn(*const u8) -> *const c_char;
type FnFailurePanicMessage = unsafe extern "C" fn(*const u8) -> *const c_char;
type FnFailureReproduceBlob = unsafe extern "C" fn(*const u8) -> *const c_char;
type FnTestCaseFromBlob = unsafe extern "C" fn(*const u8, *const c_char) -> *mut u8;
type FnTestCaseFree = unsafe extern "C" fn(*mut u8);
type FnTestCaseIsFinalReplay = unsafe extern "C" fn(*const u8) -> bool;
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
    new_pool: Symbol<'a, FnNewPool>,
    pool_add: Symbol<'a, FnPoolAdd>,
    pool_generate: Symbol<'a, FnPoolGenerate>,
    run_result_passed: Symbol<'a, FnRunResultPassed>,
    run_result_failure_count: Symbol<'a, FnRunResultFailureCount>,
    run_result_failure: Symbol<'a, FnRunResultFailure>,
    failure_origin: Symbol<'a, FnFailureOrigin>,
    failure_panic_message: Symbol<'a, FnFailurePanicMessage>,
    failure_reproduce_blob: Symbol<'a, FnFailureReproduceBlob>,
    test_case_from_blob: Symbol<'a, FnTestCaseFromBlob>,
    test_case_free: Symbol<'a, FnTestCaseFree>,
    test_case_is_final_replay: Symbol<'a, FnTestCaseIsFinalReplay>,
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
            new_pool: lib.get(b"hegel_new_pool\0").unwrap(),
            pool_add: lib.get(b"hegel_pool_add\0").unwrap(),
            pool_generate: lib.get(b"hegel_pool_generate\0").unwrap(),
            run_result_passed: lib.get(b"hegel_run_result_passed\0").unwrap(),
            run_result_failure_count: lib.get(b"hegel_run_result_failure_count\0").unwrap(),
            run_result_failure: lib.get(b"hegel_run_result_failure\0").unwrap(),
            failure_origin: lib.get(b"hegel_failure_origin\0").unwrap(),
            failure_panic_message: lib.get(b"hegel_failure_panic_message\0").unwrap(),
            failure_reproduce_blob: lib.get(b"hegel_failure_reproduce_blob\0").unwrap(),
            test_case_from_blob: lib.get(b"hegel_test_case_from_blob\0").unwrap(),
            test_case_free: lib.get(b"hegel_test_case_free\0").unwrap(),
            test_case_is_final_replay: lib.get(b"hegel_test_case_is_final_replay\0").unwrap(),
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
    integer_schema_with_order(min, max, &["type", "min_value", "max_value"])
}

/// Build the integer schema with a caller-chosen CBOR key order. Go's
/// map iteration is intentionally randomised, so a Go-emitted schema
/// hits libhegel with `max_value, type, min_value` (or any other
/// permutation) — semantically equivalent to Rust's
/// declaration-ordered emission but with different bytes. Used to
/// regression-check that the engine's schema deserializer is truly
/// order-agnostic.
fn integer_schema_with_order(min: i64, max: i64, order: &[&str]) -> Vec<u8> {
    let mut entries: Vec<(Value, Value)> = Vec::new();
    for key in order {
        let v = match *key {
            "type" => Value::Text("integer".into()),
            "min_value" => Value::Integer(min.into()),
            "max_value" => Value::Integer(max.into()),
            other => panic!("unknown schema key {other}"),
        };
        entries.push((Value::Text((*key).into()), v));
    }
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

/// HEGEL_E_INVALID_ARG from hegel.h.
const HEGEL_E_INVALID_ARG: c_int = -5;

#[test]
fn invalid_schema_returns_error_not_abort() {
    // Reproduces the hegel-java report: a plausible-but-wrong schema type
    // (`{"type":"ipv4"}`) used to `panic!("Unknown schema type")` inside the
    // engine, which — crossing the `extern "C"` boundary — aborted the host
    // process (SIGABRT). It must now return HEGEL_E_INVALID_ARG with a
    // diagnostic in hegel_last_error_message and leave the process running.
    // Under the `panic = "abort"` build (`just c-test-abort`) this test only
    // passes if no panic is reachable on the schema-interpretation path.
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 1);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 1, true);

        let run = (a.run_start)(s);
        assert!(!run.is_null());

        let tc = (a.next_test_case)(run);
        assert!(!tc.is_null(), "expected a test case");

        // Several distinct malformed schemas, each of which previously
        // panicked at a different site in the interpreter.
        let unknown_type = encode(&Value::Map(vec![(
            Value::Text("type".into()),
            Value::Text("ipv4".into()),
        )]));
        let bad_codec = encode(&Value::Map(vec![
            (Value::Text("type".into()), Value::Text("string".into())),
            (Value::Text("codec".into()), Value::Text("ebcdic".into())),
        ]));
        for bad in [unknown_type, bad_codec] {
            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(tc, bad.as_ptr(), bad.len(), &mut val_ptr, &mut val_len);
            assert_eq!(
                rc, HEGEL_E_INVALID_ARG,
                "invalid schema should return HEGEL_E_INVALID_ARG, got rc={rc}"
            );
            let err = CStr::from_ptr((a.last_error_message)()).to_string_lossy();
            assert!(
                !err.is_empty(),
                "expected a diagnostic message for the invalid schema"
            );
        }

        (a.mark_complete)(tc, CStatus::Invalid, ptr::null());
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

/// Drive the `n < 5` failing property to completion. Shared by the
/// blob tests below, which read the reproduce blob off the run result.
unsafe fn drive_failing_property(a: &Api, run: *mut u8) {
    let schema = integer_schema(0, 100);
    let origin = CString::new("n >= 5 failed").unwrap();
    loop {
        let tc = unsafe { (a.next_test_case)(run) };
        if tc.is_null() {
            break;
        }
        let mut val_ptr: *const u8 = ptr::null();
        let mut val_len: usize = 0;
        let rc = unsafe {
            (a.generate)(
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            )
        };
        if rc == -1 {
            unsafe { (a.mark_complete)(tc, CStatus::Overrun, ptr::null()) };
            continue;
        }
        assert_eq!(rc, 0, "unexpected generate rc={}", rc);
        let v = decode(unsafe { std::slice::from_raw_parts(val_ptr, val_len) });
        let Value::Integer(i) = v else {
            panic!("expected int")
        };
        let n: i128 = i.into();
        if n < 5 {
            unsafe { (a.mark_complete)(tc, CStatus::Valid, ptr::null()) };
        } else {
            unsafe { (a.mark_complete)(tc, CStatus::Interesting, origin.as_ptr()) };
        }
    }
}

/// Run the `n < 5` failing property once and return the reproduce blob of
/// its first failure. Shared by the blob-replay tests below.
unsafe fn discover_failure_blob(a: &Api) -> CString {
    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 200);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 0xc0ffee, true);

        let run = (a.run_start)(s);
        drive_failing_property(a, run);

        let result = (a.run_result)(run);
        assert!(!(a.run_result_passed)(result), "expected a failing run");
        let f = (a.run_result_failure)(result, 0);
        assert!(!f.is_null());
        let blob_ptr = (a.failure_reproduce_blob)(f);
        assert!(
            !blob_ptr.is_null(),
            "expected a reproduce blob on the failure"
        );
        // Copy out of the run-owned buffer before freeing the run.
        let blob = CStr::from_ptr(blob_ptr).to_owned();
        (a.run_free)(run);
        (a.settings_free)(s);
        blob
    }
}

/// Replay `blob` as one standalone test case and return the single drawn
/// integer of the `n < 5` property, marking the case Interesting/Valid as
/// the property dictates and freeing the handle.
unsafe fn replay_blob_once(a: &Api, s: *const u8, blob: &CStr) -> i128 {
    unsafe {
        let tc = (a.test_case_from_blob)(s, blob.as_ptr());
        assert!(
            !tc.is_null(),
            "hegel_test_case_from_blob failed: {}",
            CStr::from_ptr((a.last_error_message)()).to_string_lossy()
        );
        assert!(
            (a.test_case_is_final_replay)(tc),
            "a blob replay is the counterexample, so is_final_replay must be true"
        );

        let schema = integer_schema(0, 100);
        let mut val_ptr: *const u8 = ptr::null();
        let mut val_len: usize = 0;
        let rc = (a.generate)(
            tc,
            schema.as_ptr(),
            schema.len(),
            &mut val_ptr,
            &mut val_len,
        );
        assert_eq!(rc, 0, "unexpected generate rc={}", rc);
        let Value::Integer(i) = decode(std::slice::from_raw_parts(val_ptr, val_len)) else {
            panic!("expected int")
        };
        let n: i128 = i.into();
        // The caller plays the property's role: it alone decides whether
        // the replayed example still fails.
        if n < 5 {
            (a.mark_complete)(tc, CStatus::Valid, ptr::null());
        } else {
            let origin = CString::new("n >= 5 failed").unwrap();
            (a.mark_complete)(tc, CStatus::Interesting, origin.as_ptr());
        }
        (a.test_case_free)(tc);
        n
    }
}

#[test]
fn libhegel_blob_test_case_replays_the_counterexample() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let blob = discover_failure_blob(&a);

        // Replay the blob as standalone test cases — no run handle, no
        // worker. Replaying twice exercises the multiple-reproduce_failures
        // usage: one call per blob, each its own test case.
        let s = (a.settings_new)();
        let first = replay_blob_once(&a, s, &blob);
        assert!(
            first >= 5,
            "replayed value {first} should still violate the n < 5 property"
        );
        let second = replay_blob_once(&a, s, &blob);
        assert_eq!(first, second, "blob replay must be deterministic");
        (a.settings_free)(s);
    }
}

#[test]
fn libhegel_test_case_from_blob_rejects_bad_input() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();

        // An undecodable blob: NULL with a diagnostic.
        let garbage = CString::new("!!! not a blob !!!").unwrap();
        let tc = (a.test_case_from_blob)(s, garbage.as_ptr());
        assert!(tc.is_null());
        let err = CStr::from_ptr((a.last_error_message)()).to_string_lossy();
        assert!(
            err.contains("could not be decoded"),
            "unexpected error: {err}"
        );

        // NULL blob and NULL settings: NULL with a diagnostic.
        let tc = (a.test_case_from_blob)(s, ptr::null());
        assert!(tc.is_null());
        assert!(!CStr::from_ptr((a.last_error_message)()).is_empty());
        let blob = CString::new("AAEC").unwrap();
        let tc = (a.test_case_from_blob)(ptr::null(), blob.as_ptr());
        assert!(tc.is_null());
        assert!(!CStr::from_ptr((a.last_error_message)()).is_empty());

        (a.settings_free)(s);
    }
}

#[test]
fn libhegel_test_case_free_refuses_run_owned_test_cases() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        // Freeing NULL is a no-op.
        (a.test_case_free)(ptr::null_mut());

        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 1);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        let run = (a.run_start)(s);

        // A test case pumped from a run is owned by the run: freeing it
        // must be refused, leaving the handle usable.
        let tc = (a.next_test_case)(run);
        assert!(!tc.is_null());
        (a.test_case_free)(tc);
        let err = CStr::from_ptr((a.last_error_message)()).to_string_lossy();
        assert!(
            err.contains("owned by its hegel_run_t"),
            "unexpected error: {err}"
        );
        // Still usable after the refused free.
        assert_eq!((a.mark_complete)(tc, CStatus::Valid, ptr::null()), 0);

        (a.run_free)(run);
        (a.settings_free)(s);
    }
}

#[test]
fn libhegel_pool_primitives_draw_added_variables() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let s = (a.settings_new)();
        (a.settings_test_cases)(s, 25);
        let empty = CString::new("").unwrap();
        (a.settings_database)(s, empty.as_ptr());
        (a.settings_derandomize)(s, true);
        (a.settings_seed)(s, 3, true);

        let run = (a.run_start)(s);
        assert!(!run.is_null());

        let mut saw_pool_draw = false;
        let mut saw_empty_stop = false;
        loop {
            let tc = (a.next_test_case)(run);
            if tc.is_null() {
                let err = CStr::from_ptr((a.last_error_message)()).to_string_lossy();
                assert_eq!(err, "", "next_test_case returned NULL with error: {}", err);
                break;
            }

            // Build a pool and register three variables.
            let mut pool_id: i64 = -1;
            let rc = (a.new_pool)(tc, &mut pool_id);
            assert_eq!(rc, 0, "new_pool failed: rc={}", rc);

            let mut added = Vec::new();
            for _ in 0..3 {
                let mut var_id: i64 = -1;
                let rc = (a.pool_add)(tc, pool_id, &mut var_id);
                assert_eq!(rc, 0, "pool_add failed: rc={}", rc);
                added.push(var_id);
            }
            // pool_add hands out a fresh, strictly increasing id each time.
            assert_eq!(added, vec![1, 2, 3]);

            // Non-consuming draw: returns one of the added ids and leaves
            // the pool unchanged. `pool_generate` can report STOP_TEST if
            // the engine's choice budget is exhausted mid-shrink, so treat
            // that the same way the other primitives do.
            let mut drawn: i64 = -1;
            let rc = (a.pool_generate)(tc, pool_id, false, &mut drawn);
            if rc == -1 {
                (a.mark_complete)(tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, 0, "pool_generate failed: rc={}", rc);
            assert!(added.contains(&drawn), "drew unknown variable {}", drawn);
            saw_pool_draw = true;

            // Consume every variable, then confirm the now-empty pool
            // reports STOP_TEST on the next draw.
            let mut consumed = 0;
            for _ in 0..3 {
                let mut v: i64 = -1;
                let rc = (a.pool_generate)(tc, pool_id, true, &mut v);
                if rc == -1 {
                    break;
                }
                assert_eq!(rc, 0, "consuming pool_generate failed: rc={}", rc);
                assert!(added.contains(&v), "consumed unknown variable {}", v);
                consumed += 1;
            }
            if consumed == 3 {
                let mut v: i64 = -1;
                let rc = (a.pool_generate)(tc, pool_id, true, &mut v);
                assert_eq!(rc, -2, "expected STOP_TEST on empty pool, got rc={}", rc);
                saw_empty_stop = true;
            }

            (a.mark_complete)(tc, CStatus::Valid, ptr::null());
        }

        assert!(saw_pool_draw, "expected at least one successful pool draw");
        assert!(
            saw_empty_stop,
            "expected to drain a pool to empty at least once"
        );

        let result = (a.run_result)(run);
        assert!(!result.is_null());
        assert!((a.run_result_passed)(result), "expected passing run");

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
    //
    // This also exercises libhegel's worker-thread panic hook: the engine
    // panic must NOT print a `thread 'hegel-worker' panicked at <file>:<line>`
    // line to the test process's stderr (it's caught and surfaced through the
    // failure API instead).
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

/// Drive a `n >= 1_000_000` property through libhegel's C API across a
/// sweep of derandomized seeds. Reproduces the experiment in the
/// hegel-go shrinker-flake report, but at the libhegel boundary
/// (libloading) rather than from Go. If the engine reaches
/// `1_000_000` exactly through Rust's `embed::run_native` path (50/50
/// seeds, verified in the embed tests) but not through the C path,
/// the channel/worker shim in `hegel-c` is doing something measurable.
///
/// Run with `--ignored` because it's a 50-seed loop and adds ~10s to
/// the smoke suite.
#[test]
#[ignore = "shrinker sweep — slow; run via --ignored for diagnostics"]
fn shrinker_reaches_boundary_via_c_api_sweep() {
    shrinker_sweep_with_schema_order(&["type", "min_value", "max_value"], "rust-order");
}

/// Same sweep with Go's map-iteration-style key ordering. If hit rate
/// differs from the Rust-order sweep, schema deserialization in the
/// engine is order-sensitive somewhere.
#[test]
#[ignore = "shrinker sweep — slow; run via --ignored for diagnostics"]
fn shrinker_reaches_boundary_via_c_api_sweep_go_key_order() {
    shrinker_sweep_with_schema_order(&["max_value", "type", "min_value"], "go-order");
}

/// Characterization test for the origin contract: when the caller
/// passes a *unique origin per failing draw* (e.g. when a binding
/// uses `panic(fmt.Sprintf("n=%d", n))` and forwards the panic
/// message as origin), the engine treats each as a distinct bug and
/// partitions its shrink budget across them. This test exists so a
/// future change to that partitioning behavior is loud — and so a
/// binding-author searching the codebase for `unique origins` finds
/// the explanation in one place.
///
/// Concretely: holding the schema and seed range constant, switching
/// from a stable per-bug origin to a per-value origin drops the
/// boundary hit rate from ~100/100 to ~16/100. The Rust-side embed
/// API (`hegel::embed::run_native`) does not have this problem
/// because hegel-rust's panic handler derives origin from panic
/// *location*, not value (`format!("Panic at {}", location)` in
/// `src/run_lifecycle.rs`).
///
/// Bindings that want hegel-rust-like behavior should derive origin
/// from the panic source location (file:line) rather than the panic
/// message.
#[test]
#[ignore = "shrinker characterization — slow; run via --ignored for diagnostics"]
fn shrinker_partitions_budget_across_unique_origins() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };
    let (hits, _) = run_shrinker_sweep(
        &a,
        &["type", "min_value", "max_value"],
        OriginStyle::PerDrawValue,
        1..=100,
    );
    eprintln!("[unique-origins] boundary hit rate: {hits}/100");
    // The exact rate varies with shrinker tuning; just assert it's
    // markedly worse than the stable-origin case (100/100). A
    // regression that pushed this above 70/100 would mean either the
    // shrinker stopped partitioning by origin (unlikely; documented
    // contract) or the test stopped exercising the partitioning path.
    assert!(
        hits < 70,
        "expected partitioned-budget hit rate to be well below stable-origin's 100/100, got {hits}/100"
    );
}

#[derive(Copy, Clone)]
enum OriginStyle {
    Constant,
    PerDrawValue,
}

fn shrinker_sweep_with_schema_order(order: &[&str], label: &str) {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };
    let (hits, observed) = run_shrinker_sweep(&a, order, OriginStyle::Constant, 1..=100);
    eprintln!("[{label}] C-API shrinker boundary hit rate: {hits}/100");
    eprintln!("[{label}] values: {observed:?}");
    assert!(
        hits >= 50,
        "[{label}] C-API shrinker only reached boundary {hits}/100; values: {observed:?}"
    );
}

fn run_shrinker_sweep(
    a: &Api<'_>,
    order: &[&str],
    origin_style: OriginStyle,
    seeds: std::ops::RangeInclusive<u64>,
) -> (u32, Vec<i128>) {
    let schema = integer_schema_with_order(i64::MIN, i64::MAX, order);
    let empty = CString::new("").unwrap();
    let constant_origin = CString::new("n >= 1_000_000").unwrap();

    let mut hits = 0u32;
    let mut observed = Vec::<i128>::new();
    for seed in seeds {
        let mut last_failing: Option<i128> = None;
        unsafe {
            let s = (a.settings_new)();
            (a.settings_test_cases)(s, 100);
            (a.settings_database)(s, empty.as_ptr());
            (a.settings_derandomize)(s, true);
            (a.settings_seed)(s, seed, true);

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
                if n >= 1_000_000 {
                    last_failing = Some(n);
                    let origin_cs: CString;
                    let origin_ptr = match origin_style {
                        OriginStyle::Constant => constant_origin.as_ptr(),
                        OriginStyle::PerDrawValue => {
                            origin_cs = CString::new(format!("n={n}")).unwrap();
                            origin_cs.as_ptr()
                        }
                    };
                    (a.mark_complete)(tc, CStatus::Interesting, origin_ptr);
                } else {
                    (a.mark_complete)(tc, CStatus::Valid, ptr::null());
                }
            }
            (a.run_free)(run);
            (a.settings_free)(s);
        }
        let final_value = last_failing.unwrap_or(i128::MIN);
        observed.push(final_value);
        if final_value == 1_000_000 {
            hits += 1;
        }
    }
    (hits, observed)
}
