use std::ffi::{CStr, CString, c_char, c_int};
use std::path::PathBuf;
use std::ptr;

use ciborium::Value;
use libloading::{Library, Symbol};

/// HEGEL_OK from hegel.h.
const HEGEL_OK: c_int = 0;
/// HEGEL_E_STOP_TEST from hegel.h.
const HEGEL_E_STOP_TEST: c_int = -1;
/// HEGEL_E_ASSUME from hegel.h.
const HEGEL_E_ASSUME: c_int = -2;
/// HEGEL_E_INVALID_HANDLE from hegel.h.
const HEGEL_E_INVALID_HANDLE: c_int = -4;
/// HEGEL_E_INVALID_ARG from hegel.h.
const HEGEL_E_INVALID_ARG: c_int = -5;
/// HEGEL_E_NOT_COMPLETE from hegel.h.
const HEGEL_E_NOT_COMPLETE: c_int = -7;

fn lib_path() -> PathBuf {
    let filename = if cfg!(target_os = "macos") {
        "libhegel_c.dylib"
    } else if cfg!(target_os = "windows") {
        "hegel_c.dll"
    } else {
        "libhegel_c.so"
    };
    if let Ok(dir) = std::env::var("HEGEL_C_LIB_DIR") {
        let candidate = PathBuf::from(dir).join(filename);
        assert!(
            candidate.exists(),
            "HEGEL_C_LIB_DIR is set but {} does not exist",
            candidate.display()
        );
        return candidate;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(profile_dir) = exe.parent().and_then(|deps| deps.parent()) {
            let candidate = profile_dir.join(filename);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.parent().unwrap().join("target");
    for profile in ["debug", "release"] {
        let candidate = target_dir.join(profile).join(filename);
        if candidate.exists() {
            return candidate;
        }
    }
    panic!(
        "could not find {} near {} or under {}; run `cargo build -p hegeltest-c` first",
        filename,
        std::env::current_exe()
            .ok()
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown exe>".to_string()),
        target_dir.display()
    );
}

unsafe fn load() -> Library {
    unsafe { Library::new(lib_path()).expect("dlopen libhegel") }
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum CStatus {
    Valid = 0,
    Invalid = 1,
    Overrun = 2,
    Interesting = 3,
}

/// `hegel_run_status_t` from hegel.h.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum CRunStatus {
    Passed = 0,
    Failed = 1,
    Error = 2,
}

/// `hegel_backend_t` from hegel.h.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum CBackend {
    Auto = 0,
    Default = 1,
    Urandom = 2,
}

type FnContextNew = unsafe extern "C" fn() -> *mut u8;
type FnContextFree = unsafe extern "C" fn(*mut u8) -> c_int;
type FnContextLastError = unsafe extern "C" fn(*const u8) -> *const c_char;
type FnSettingsNew = unsafe extern "C" fn(*mut u8, *mut *mut u8) -> c_int;
type FnSettingsFree = unsafe extern "C" fn(*mut u8, *mut u8) -> c_int;
type FnSettingsTestCases = unsafe extern "C" fn(*mut u8, *mut u8, u64) -> c_int;
type FnSettingsDatabase = unsafe extern "C" fn(*mut u8, *mut u8, *const c_char) -> c_int;
type FnSettingsDatabaseKey = unsafe extern "C" fn(*mut u8, *mut u8, *const c_char) -> c_int;
type FnSettingsSeed = unsafe extern "C" fn(*mut u8, *mut u8, u64, bool) -> c_int;
type FnSettingsDerandomize = unsafe extern "C" fn(*mut u8, *mut u8, bool) -> c_int;
type FnSettingsBackend = unsafe extern "C" fn(*mut u8, *mut u8, CBackend) -> c_int;
type FnRunStart = unsafe extern "C" fn(*mut u8, *const u8, *mut *mut u8) -> c_int;
type FnNextTestCase = unsafe extern "C" fn(*mut u8, *mut u8, *mut *mut u8) -> c_int;
type FnRunResult = unsafe extern "C" fn(*mut u8, *mut u8, *mut *mut u8) -> c_int;
type FnRunResultFree = unsafe extern "C" fn(*mut u8, *mut u8) -> c_int;
type FnFailureFree = unsafe extern "C" fn(*mut u8, *mut u8) -> c_int;
type FnRunFree = unsafe extern "C" fn(*mut u8, *mut u8) -> c_int;
type FnGenerate =
    unsafe extern "C" fn(*mut u8, *mut u8, *const u8, usize, *mut *const u8, *mut usize) -> c_int;
type FnMarkComplete = unsafe extern "C" fn(*mut u8, *mut u8, CStatus, *const c_char) -> c_int;
type FnNewPool = unsafe extern "C" fn(*mut u8, *mut u8, *mut i64) -> c_int;
type FnPoolAdd = unsafe extern "C" fn(*mut u8, *mut u8, i64, *mut i64) -> c_int;
type FnPoolGenerate = unsafe extern "C" fn(*mut u8, *mut u8, i64, bool, *mut i64) -> c_int;
type FnNewStateMachine = unsafe extern "C" fn(
    *mut u8,
    *mut u8,
    *const *const c_char,
    usize,
    *const *const c_char,
    usize,
    *mut i64,
) -> c_int;
type FnStateMachineNextRule = unsafe extern "C" fn(*mut u8, *mut u8, i64, *mut u64) -> c_int;
type FnPrimitiveBoolean =
    unsafe extern "C" fn(*mut u8, *mut u8, f64, bool, bool, *mut bool) -> c_int;
type FnTarget = unsafe extern "C" fn(*mut u8, *mut u8, f64, *const c_char) -> c_int;
type FnCollectionMore = unsafe extern "C" fn(*mut u8, *mut u8, i64, *mut bool) -> c_int;
type FnRunResultStatus = unsafe extern "C" fn(*mut u8, *const u8, *mut CRunStatus) -> c_int;
type FnRunResultError = unsafe extern "C" fn(*mut u8, *const u8, *mut *const c_char) -> c_int;
type FnRunResultFailureCount = unsafe extern "C" fn(*mut u8, *const u8, *mut usize) -> c_int;
type FnRunResultFailure = unsafe extern "C" fn(*mut u8, *const u8, usize, *mut *mut u8) -> c_int;
type FnFailureOrigin = unsafe extern "C" fn(*mut u8, *const u8, *mut *const c_char) -> c_int;
type FnFailureReproduceBlob = unsafe extern "C" fn(*mut u8, *const u8, *mut *const c_char) -> c_int;
type FnTestCaseFromBlob =
    unsafe extern "C" fn(*mut u8, *const u8, *const c_char, *mut *mut u8) -> c_int;
type FnTestCaseFree = unsafe extern "C" fn(*mut u8, *mut u8) -> c_int;

struct Api<'a> {
    context_new: Symbol<'a, FnContextNew>,
    context_free: Symbol<'a, FnContextFree>,
    context_last_error: Symbol<'a, FnContextLastError>,
    settings_new: Symbol<'a, FnSettingsNew>,
    settings_free: Symbol<'a, FnSettingsFree>,
    settings_test_cases: Symbol<'a, FnSettingsTestCases>,
    settings_database: Symbol<'a, FnSettingsDatabase>,
    settings_database_key: Symbol<'a, FnSettingsDatabaseKey>,
    settings_seed: Symbol<'a, FnSettingsSeed>,
    settings_derandomize: Symbol<'a, FnSettingsDerandomize>,
    settings_backend: Symbol<'a, FnSettingsBackend>,
    run_start: Symbol<'a, FnRunStart>,
    next_test_case: Symbol<'a, FnNextTestCase>,
    run_result: Symbol<'a, FnRunResult>,
    run_free: Symbol<'a, FnRunFree>,
    generate: Symbol<'a, FnGenerate>,
    mark_complete: Symbol<'a, FnMarkComplete>,
    new_pool: Symbol<'a, FnNewPool>,
    pool_add: Symbol<'a, FnPoolAdd>,
    pool_generate: Symbol<'a, FnPoolGenerate>,
    new_state_machine: Symbol<'a, FnNewStateMachine>,
    state_machine_next_rule: Symbol<'a, FnStateMachineNextRule>,
    primitive_boolean: Symbol<'a, FnPrimitiveBoolean>,
    target: Symbol<'a, FnTarget>,
    collection_more: Symbol<'a, FnCollectionMore>,
    run_result_status: Symbol<'a, FnRunResultStatus>,
    run_result_error: Symbol<'a, FnRunResultError>,
    run_result_failure_count: Symbol<'a, FnRunResultFailureCount>,
    run_result_failure: Symbol<'a, FnRunResultFailure>,
    run_result_free: Symbol<'a, FnRunResultFree>,
    failure_origin: Symbol<'a, FnFailureOrigin>,
    failure_reproduce_blob: Symbol<'a, FnFailureReproduceBlob>,
    failure_free: Symbol<'a, FnFailureFree>,
    test_case_from_blob: Symbol<'a, FnTestCaseFromBlob>,
    test_case_free: Symbol<'a, FnTestCaseFree>,
}

unsafe fn bind(lib: &Library) -> Api<'_> {
    unsafe {
        Api {
            context_new: lib.get(b"hegel_context_new\0").unwrap(),
            context_free: lib.get(b"hegel_context_free\0").unwrap(),
            context_last_error: lib.get(b"hegel_context_last_error\0").unwrap(),
            settings_new: lib.get(b"hegel_settings_new\0").unwrap(),
            settings_free: lib.get(b"hegel_settings_free\0").unwrap(),
            settings_test_cases: lib.get(b"hegel_settings_set_test_cases\0").unwrap(),
            settings_database: lib.get(b"hegel_settings_set_database\0").unwrap(),
            settings_database_key: lib.get(b"hegel_settings_set_database_key\0").unwrap(),
            settings_seed: lib.get(b"hegel_settings_set_seed\0").unwrap(),
            settings_derandomize: lib.get(b"hegel_settings_set_derandomize\0").unwrap(),
            settings_backend: lib.get(b"hegel_settings_set_backend\0").unwrap(),
            run_start: lib.get(b"hegel_run_start\0").unwrap(),
            next_test_case: lib.get(b"hegel_next_test_case\0").unwrap(),
            run_result: lib.get(b"hegel_run_result\0").unwrap(),
            run_free: lib.get(b"hegel_run_free\0").unwrap(),
            generate: lib.get(b"hegel_generate\0").unwrap(),
            mark_complete: lib.get(b"hegel_mark_complete\0").unwrap(),
            new_pool: lib.get(b"hegel_new_pool\0").unwrap(),
            pool_add: lib.get(b"hegel_pool_add\0").unwrap(),
            pool_generate: lib.get(b"hegel_pool_generate\0").unwrap(),
            new_state_machine: lib.get(b"hegel_new_state_machine\0").unwrap(),
            state_machine_next_rule: lib.get(b"hegel_state_machine_next_rule\0").unwrap(),
            primitive_boolean: lib.get(b"hegel_primitive_boolean\0").unwrap(),
            target: lib.get(b"hegel_target\0").unwrap(),
            collection_more: lib.get(b"hegel_collection_more\0").unwrap(),
            run_result_status: lib.get(b"hegel_run_result_status\0").unwrap(),
            run_result_error: lib.get(b"hegel_run_result_error\0").unwrap(),
            run_result_failure_count: lib.get(b"hegel_run_result_failure_count\0").unwrap(),
            run_result_failure: lib.get(b"hegel_run_result_failure\0").unwrap(),
            run_result_free: lib.get(b"hegel_run_result_free\0").unwrap(),
            failure_origin: lib.get(b"hegel_failure_origin\0").unwrap(),
            failure_reproduce_blob: lib.get(b"hegel_failure_reproduction_blob\0").unwrap(),
            failure_free: lib.get(b"hegel_failure_free\0").unwrap(),
            test_case_from_blob: lib.get(b"hegel_test_case_from_blob\0").unwrap(),
            test_case_free: lib.get(b"hegel_test_case_free\0").unwrap(),
        }
    }
}

impl Api<'_> {
    /// Panic with the context's diagnostic if `rc` is not `HEGEL_OK`.
    unsafe fn expect_ok(&self, ctx: *const u8, rc: c_int, what: &str) {
        if rc != HEGEL_OK {
            let p = unsafe { self.context_last_error(ctx) };
            let msg = if p.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            };
            panic!("{what} failed: rc={rc} {msg}");
        }
    }
    unsafe fn settings_new(&self, ctx: *mut u8) -> *mut u8 {
        let mut s: *mut u8 = ptr::null_mut();
        let rc = unsafe { (self.settings_new)(ctx, &mut s) };
        unsafe { self.expect_ok(ctx, rc, "hegel_settings_new") };
        s
    }
    unsafe fn settings_free(&self, ctx: *mut u8, s: *mut u8) {
        let rc = unsafe { (self.settings_free)(ctx, s) };
        unsafe { self.expect_ok(ctx, rc, "hegel_settings_free") };
    }
    unsafe fn settings_test_cases(&self, ctx: *mut u8, s: *mut u8, n: u64) {
        let rc = unsafe { (self.settings_test_cases)(ctx, s, n) };
        unsafe { self.expect_ok(ctx, rc, "hegel_settings_set_test_cases") };
    }
    unsafe fn settings_seed(&self, ctx: *mut u8, s: *mut u8, seed: u64, has_seed: bool) {
        let rc = unsafe { (self.settings_seed)(ctx, s, seed, has_seed) };
        unsafe { self.expect_ok(ctx, rc, "hegel_settings_set_seed") };
    }
    unsafe fn settings_derandomize(&self, ctx: *mut u8, s: *mut u8, derandomize: bool) {
        let rc = unsafe { (self.settings_derandomize)(ctx, s, derandomize) };
        unsafe { self.expect_ok(ctx, rc, "hegel_settings_set_derandomize") };
    }
    unsafe fn settings_backend(&self, ctx: *mut u8, s: *mut u8, backend: CBackend) {
        let rc = unsafe { (self.settings_backend)(ctx, s, backend) };
        unsafe { self.expect_ok(ctx, rc, "hegel_settings_set_backend") };
    }
    unsafe fn run_start(&self, ctx: *mut u8, s: *const u8) -> *mut u8 {
        let mut run: *mut u8 = ptr::null_mut();
        let rc = unsafe { (self.run_start)(ctx, s, &mut run) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_start") };
        run
    }
    /// Null at clean completion (`HEGEL_OK` with no case); any error code is a
    /// misuse and panics. The deliberate-misuse test calls the raw symbol.
    unsafe fn next_test_case(&self, ctx: *mut u8, run: *mut u8) -> *mut u8 {
        let mut tc: *mut u8 = ptr::null_mut();
        let rc = unsafe { (self.next_test_case)(ctx, run, &mut tc) };
        unsafe { self.expect_ok(ctx, rc, "hegel_next_test_case") };
        tc
    }
    unsafe fn run_result(&self, ctx: *mut u8, run: *mut u8) -> *mut u8 {
        let mut r: *mut u8 = ptr::null_mut();
        let rc = unsafe { (self.run_result)(ctx, run, &mut r) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_result") };
        r
    }
    unsafe fn run_result_free(&self, ctx: *mut u8, r: *mut u8) {
        let rc = unsafe { (self.run_result_free)(ctx, r) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_result_free") };
    }
    unsafe fn failure_free(&self, ctx: *mut u8, f: *mut u8) {
        let rc = unsafe { (self.failure_free)(ctx, f) };
        unsafe { self.expect_ok(ctx, rc, "hegel_failure_free") };
    }
    unsafe fn run_free(&self, ctx: *mut u8, run: *mut u8) {
        let rc = unsafe { (self.run_free)(ctx, run) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_free") };
    }
    unsafe fn mark_complete(
        &self,
        ctx: *mut u8,
        tc: *mut u8,
        status: CStatus,
        origin: *const c_char,
    ) {
        let rc = unsafe { (self.mark_complete)(ctx, tc, status, origin) };
        unsafe { self.expect_ok(ctx, rc, "hegel_mark_complete") };
    }
    /// Complete a case and then release the caller's handle, which every handle
    /// (run-owned included) now requires. Used wherever a case is concluded and
    /// not touched again.
    unsafe fn complete_and_free(
        &self,
        ctx: *mut u8,
        tc: *mut u8,
        status: CStatus,
        origin: *const c_char,
    ) {
        unsafe { self.mark_complete(ctx, tc, status, origin) };
        unsafe { (self.test_case_free)(ctx, tc) };
    }
    /// The error-reporting reader: returns the message pointer directly (NULL
    /// for a NULL context), so it forwards the raw symbol unchanged.
    unsafe fn context_last_error(&self, ctx: *const u8) -> *const c_char {
        unsafe { (self.context_last_error)(ctx) }
    }
    unsafe fn run_result_status(&self, ctx: *mut u8, r: *const u8) -> CRunStatus {
        let mut st = CRunStatus::Passed;
        let rc = unsafe { (self.run_result_status)(ctx, r, &mut st) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_result_status") };
        st
    }
    unsafe fn run_result_error(&self, ctx: *mut u8, r: *const u8) -> *const c_char {
        let mut p: *const c_char = ptr::null();
        let rc = unsafe { (self.run_result_error)(ctx, r, &mut p) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_result_error") };
        p
    }
    unsafe fn run_result_failure_count(&self, ctx: *mut u8, r: *const u8) -> usize {
        let mut n = 0usize;
        let rc = unsafe { (self.run_result_failure_count)(ctx, r, &mut n) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_result_failure_count") };
        n
    }
    unsafe fn run_result_failure(&self, ctx: *mut u8, r: *const u8, index: usize) -> *mut u8 {
        let mut f: *mut u8 = ptr::null_mut();
        let rc = unsafe { (self.run_result_failure)(ctx, r, index, &mut f) };
        unsafe { self.expect_ok(ctx, rc, "hegel_run_result_failure") };
        f
    }
    unsafe fn failure_origin(&self, ctx: *mut u8, f: *const u8) -> *const c_char {
        let mut p: *const c_char = ptr::null();
        let rc = unsafe { (self.failure_origin)(ctx, f, &mut p) };
        unsafe { self.expect_ok(ctx, rc, "hegel_failure_origin") };
        p
    }
    unsafe fn failure_reproduce_blob(&self, ctx: *mut u8, f: *const u8) -> *const c_char {
        let mut p: *const c_char = ptr::null();
        let rc = unsafe { (self.failure_reproduce_blob)(ctx, f, &mut p) };
        unsafe { self.expect_ok(ctx, rc, "hegel_failure_reproduction_blob") };
        p
    }
    /// Null on failure (bad / undecodable blob); the diagnostic is on the
    /// context. The bad-input test relies on the null, so this does not check.
    unsafe fn test_case_from_blob(
        &self,
        ctx: *mut u8,
        s: *const u8,
        blob: *const c_char,
    ) -> *mut u8 {
        let mut tc: *mut u8 = ptr::null_mut();
        unsafe { (self.test_case_from_blob)(ctx, s, blob, &mut tc) };
        tc
    }
}

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

#[test]
fn libhegel_runs_passing_property() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        assert!(!s.is_null());
        a.settings_test_cases(ctx, s, 25);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 1, true);

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());

        let schema = integer_schema(0, 100);
        let mut cases = 0usize;
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
                assert_eq!(err, "", "next_test_case returned NULL with error: {}", err);
                break;
            }
            cases += 1;

            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                ctx,
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            assert_eq!(rc, HEGEL_OK, "generate failed: rc={}", rc);

            let val_bytes = std::slice::from_raw_parts(val_ptr, val_len);
            let v = decode(val_bytes);
            if let Value::Integer(i) = v {
                let n: i128 = i.into();
                assert!((0..=100).contains(&n), "got out-of-range value {}", n);
            } else {
                panic!("expected integer, got {:?}", v);
            }

            a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        }
        assert!(cases >= 1, "expected at least one test case to run");

        let result = a.run_result(ctx, run);
        assert!(!result.is_null(), "run_result null after drained loop");
        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Passed,
            "expected passing run"
        );
        assert_eq!(a.run_result_failure_count(ctx, result), 0);
        assert!(
            a.run_result_error(ctx, result).is_null(),
            "a normal run carries no run-level error"
        );
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

/// Pinning the urandom backend via `hegel_settings_set_backend` drives a run to
/// completion through the urandom RNG path (rather than the default PRNG).
/// A trivial always-valid property still passes; this just exercises the
/// new setter end-to-end and confirms it wires through to a working run.
#[test]
fn libhegel_runs_with_urandom_backend() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        assert!(!s.is_null());
        a.settings_test_cases(ctx, s, 10);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_backend(ctx, s, CBackend::Urandom);

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());

        let schema = integer_schema(0, 100);
        let mut cases = 0usize;
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
                assert_eq!(err, "", "next_test_case returned NULL with error: {}", err);
                break;
            }
            cases += 1;

            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                ctx,
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            assert_eq!(rc, 0, "generate failed: rc={}", rc);

            let v = decode(std::slice::from_raw_parts(val_ptr, val_len));
            let Value::Integer(i) = v else {
                panic!("expected integer, got {:?}", v)
            };
            let n: i128 = i.into();
            assert!((0..=100).contains(&n), "got out-of-range value {}", n);

            a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        }
        assert!(cases >= 1, "expected at least one test case to run");

        let result = a.run_result(ctx, run);
        assert!(!result.is_null());
        assert_eq!(a.run_result_status(ctx, result), CRunStatus::Passed);
        assert_eq!(a.run_result_failure_count(ctx, result), 0);
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn invalid_schema_returns_error_not_abort() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 1);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 1, true);

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());

        let tc = a.next_test_case(ctx, run);
        assert!(!tc.is_null(), "expected a test case");

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
            let rc = (a.generate)(ctx, tc, bad.as_ptr(), bad.len(), &mut val_ptr, &mut val_len);
            assert_eq!(
                rc, HEGEL_E_INVALID_ARG,
                "invalid schema should return HEGEL_E_INVALID_ARG, got rc={rc}"
            );
            let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
            assert!(
                !err.is_empty(),
                "expected a diagnostic message for the invalid schema"
            );
        }

        a.complete_and_free(ctx, tc, CStatus::Invalid, ptr::null());
        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn caller_usage_errors_return_error_not_abort() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 1);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 1, true);

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());
        let tc = a.next_test_case(ctx, run);
        assert!(!tc.is_null(), "expected a test case");

        let label = CString::new("x").unwrap();
        let dup = CString::new("dup").unwrap();

        assert_eq!(
            (a.target)(ctx, tc, f64::NAN, label.as_ptr()),
            HEGEL_E_INVALID_ARG
        );
        assert!(
            !CStr::from_ptr(a.context_last_error(ctx))
                .to_bytes()
                .is_empty()
        );
        assert_eq!((a.target)(ctx, tc, 1.0, dup.as_ptr()), HEGEL_OK);
        assert_eq!((a.target)(ctx, tc, 2.0, dup.as_ptr()), HEGEL_E_INVALID_ARG);

        let mut more = false;
        assert_eq!(
            (a.collection_more)(ctx, tc, 9999, &mut more),
            HEGEL_E_INVALID_ARG
        );
        let mut var_id = 0i64;
        assert_eq!(
            (a.pool_add)(ctx, tc, 9999, &mut var_id),
            HEGEL_E_INVALID_ARG
        );
        let mut rule_idx = 0u64;
        assert_eq!(
            (a.state_machine_next_rule)(ctx, tc, 9999, &mut rule_idx),
            HEGEL_E_INVALID_ARG
        );

        a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn libhegel_reports_shrunk_failure() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 200);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 0xc0ffee, true);

        let run = a.run_start(ctx, s);
        let schema = integer_schema(0, 100);
        let origin = CString::new("n >= 5 failed").unwrap();

        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
                assert_eq!(err, "", "got error mid-loop: {}", err);
                break;
            }

            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                ctx,
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            if rc == HEGEL_E_STOP_TEST {
                a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, HEGEL_OK, "unexpected generate rc={}", rc);

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
            a.complete_and_free(ctx, tc, status, origin_ptr);
        }

        let result = a.run_result(ctx, run);
        assert!(!result.is_null());
        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Failed,
            "expected failing run (predicate n < 5 is false for many n in [0,100])"
        );
        let n_failures = a.run_result_failure_count(ctx, result);
        assert!(n_failures >= 1, "expected at least one failure");

        let f = a.run_result_failure(ctx, result, 0);
        assert!(!f.is_null());
        let origin_back = CStr::from_ptr(a.failure_origin(ctx, f)).to_string_lossy();
        assert!(
            origin_back.contains("n >= 5 failed"),
            "expected failure origin to contain 'n >= 5 failed', got: {}",
            origin_back
        );
        a.failure_free(ctx, f);
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

/// Drive the `n < 5` failing property to completion. Shared by the
/// blob tests below, which read the reproduce blob off the run result.
unsafe fn drive_failing_property(a: &Api, ctx: *mut u8, run: *mut u8) {
    let schema = integer_schema(0, 100);
    let origin = CString::new("n >= 5 failed").unwrap();
    loop {
        let tc = unsafe { a.next_test_case(ctx, run) };
        if tc.is_null() {
            break;
        }
        let mut val_ptr: *const u8 = ptr::null();
        let mut val_len: usize = 0;
        let rc = unsafe {
            (a.generate)(
                ctx,
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            )
        };
        if rc == HEGEL_E_STOP_TEST {
            unsafe { a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null()) };
            continue;
        }
        assert_eq!(rc, HEGEL_OK, "unexpected generate rc={}", rc);
        let v = decode(unsafe { std::slice::from_raw_parts(val_ptr, val_len) });
        let Value::Integer(i) = v else {
            panic!("expected int")
        };
        let n: i128 = i.into();
        if n < 5 {
            unsafe { a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null()) };
        } else {
            unsafe { a.complete_and_free(ctx, tc, CStatus::Interesting, origin.as_ptr()) };
        }
    }
}

/// Run the `n < 5` failing property once and return the reproduce blob of
/// its first failure. Shared by the blob-replay tests below.
unsafe fn discover_failure_blob(a: &Api, ctx: *mut u8) -> CString {
    unsafe {
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 200);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 0xc0ffee, true);

        let run = a.run_start(ctx, s);
        drive_failing_property(a, ctx, run);

        let result = a.run_result(ctx, run);
        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Failed,
            "expected a failing run"
        );
        let f = a.run_result_failure(ctx, result, 0);
        assert!(!f.is_null());
        let blob_ptr = a.failure_reproduce_blob(ctx, f);
        assert!(
            !blob_ptr.is_null(),
            "expected a reproduce blob on the failure"
        );
        let blob = CStr::from_ptr(blob_ptr).to_owned();
        a.failure_free(ctx, f);
        a.run_result_free(ctx, result);
        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        blob
    }
}

/// Replay `blob` as one standalone test case and return the single drawn
/// integer of the `n < 5` property, marking the case Interesting/Valid as
/// the property dictates and freeing the handle.
unsafe fn replay_blob_once(a: &Api, ctx: *mut u8, s: *const u8, blob: &CStr) -> i128 {
    unsafe {
        let tc = a.test_case_from_blob(ctx, s, blob.as_ptr());
        assert!(
            !tc.is_null(),
            "hegel_test_case_from_blob failed: {}",
            CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy()
        );

        let schema = integer_schema(0, 100);
        let mut val_ptr: *const u8 = ptr::null();
        let mut val_len: usize = 0;
        let rc = (a.generate)(
            ctx,
            tc,
            schema.as_ptr(),
            schema.len(),
            &mut val_ptr,
            &mut val_len,
        );
        assert_eq!(rc, HEGEL_OK, "unexpected generate rc={}", rc);
        let Value::Integer(i) = decode(std::slice::from_raw_parts(val_ptr, val_len)) else {
            panic!("expected int")
        };
        let n: i128 = i.into();
        if n < 5 {
            a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        } else {
            let origin = CString::new("n >= 5 failed").unwrap();
            a.complete_and_free(ctx, tc, CStatus::Interesting, origin.as_ptr());
        }
        n
    }
}

#[test]
fn libhegel_blob_test_case_replays_the_counterexample() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let blob = discover_failure_blob(&a, ctx);

        let s = a.settings_new(ctx);
        let first = replay_blob_once(&a, ctx, s, &blob);
        assert!(
            first >= 5,
            "replayed value {first} should still violate the n < 5 property"
        );
        let second = replay_blob_once(&a, ctx, s, &blob);
        assert_eq!(first, second, "blob replay must be deterministic");
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn libhegel_test_case_from_blob_rejects_bad_input() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);

        let garbage = CString::new("!!! not a blob !!!").unwrap();
        let tc = a.test_case_from_blob(ctx, s, garbage.as_ptr());
        assert!(tc.is_null());
        let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
        assert!(
            err.contains("could not be decoded"),
            "unexpected error: {err}"
        );

        let tc = a.test_case_from_blob(ctx, s, ptr::null());
        assert!(tc.is_null());
        assert!(!CStr::from_ptr(a.context_last_error(ctx)).is_empty());
        let blob = CString::new("AAEC").unwrap();
        let tc = a.test_case_from_blob(ctx, ptr::null(), blob.as_ptr());
        assert!(tc.is_null());
        assert!(!CStr::from_ptr(a.context_last_error(ctx)).is_empty());

        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn libhegel_frees_run_owned_test_cases() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        (a.test_case_free)(ctx, ptr::null_mut());

        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 1);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        let run = a.run_start(ctx, s);

        let tc = a.next_test_case(ctx, run);
        assert!(!tc.is_null());
        // A run-owned handle is owned by the caller and released with
        // `hegel_test_case_free` like any other; the run keeps its own
        // reference, so freeing the handle here (without completing the case)
        // does not disturb the run, which winds it down on `run_free`.
        (a.test_case_free)(ctx, tc);
        let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
        assert!(
            err.is_empty(),
            "freeing a run-owned handle should succeed: {err}"
        );

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn libhegel_pool_primitives_draw_added_variables() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 25);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 3, true);

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());

        let mut saw_pool_draw = false;
        let mut saw_empty_reject = false;
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
                assert_eq!(err, "", "next_test_case returned NULL with error: {}", err);
                break;
            }

            let mut pool_id: i64 = -1;
            let rc = (a.new_pool)(ctx, tc, &mut pool_id);
            assert_eq!(rc, HEGEL_OK, "new_pool failed: rc={}", rc);

            let mut added = Vec::new();
            for _ in 0..3 {
                let mut var_id: i64 = -1;
                let rc = (a.pool_add)(ctx, tc, pool_id, &mut var_id);
                assert_eq!(rc, HEGEL_OK, "pool_add failed: rc={}", rc);
                added.push(var_id);
            }
            assert_eq!(added, vec![1, 2, 3]);

            let mut drawn: i64 = -1;
            let rc = (a.pool_generate)(ctx, tc, pool_id, false, &mut drawn);
            if rc == HEGEL_E_STOP_TEST {
                a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, HEGEL_OK, "pool_generate failed: rc={}", rc);
            assert!(added.contains(&drawn), "drew unknown variable {}", drawn);
            saw_pool_draw = true;

            let mut consumed = 0;
            for _ in 0..3 {
                let mut v: i64 = -1;
                let rc = (a.pool_generate)(ctx, tc, pool_id, true, &mut v);
                if rc == HEGEL_E_STOP_TEST {
                    break;
                }
                assert_eq!(rc, HEGEL_OK, "consuming pool_generate failed: rc={}", rc);
                assert!(added.contains(&v), "consumed unknown variable {}", v);
                consumed += 1;
            }
            if consumed == 3 {
                let mut v: i64 = -1;
                let rc = (a.pool_generate)(ctx, tc, pool_id, true, &mut v);
                assert_eq!(
                    rc, HEGEL_E_ASSUME,
                    "expected HEGEL_E_ASSUME on empty pool, got rc={}",
                    rc
                );
                saw_empty_reject = true;
            }

            a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        }

        assert!(saw_pool_draw, "expected at least one successful pool draw");
        assert!(
            saw_empty_reject,
            "expected to drain a pool to empty at least once"
        );

        let result = a.run_result(ctx, run);
        assert!(!result.is_null());
        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Passed,
            "expected passing run"
        );
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn libhegel_state_machine_selects_registered_rules_with_swarm() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 50);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 3, true);

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());

        let rule_names: Vec<CString> = ["push", "pop", "clear"]
            .iter()
            .map(|n| CString::new(*n).unwrap())
            .collect();
        let rule_ptrs: Vec<*const c_char> = rule_names.iter().map(|n| n.as_ptr()).collect();
        let invariant_name = CString::new("sorted").unwrap();
        let invariant_ptrs = [invariant_name.as_ptr()];

        let mut saw_rule_draw = false;
        let mut longest_single_rule_run = 0usize;
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
                assert_eq!(err, "", "next_test_case returned NULL with error: {}", err);
                break;
            }

            let mut machine_id: i64 = -1;
            let rc = (a.new_state_machine)(
                ctx,
                tc,
                ptr::null(),
                0,
                invariant_ptrs.as_ptr(),
                invariant_ptrs.len(),
                &mut machine_id,
            );
            assert_eq!(
                rc, HEGEL_E_INVALID_ARG,
                "expected HEGEL_E_INVALID_ARG, got rc={}",
                rc
            );

            let rc = (a.new_state_machine)(
                ctx,
                tc,
                rule_ptrs.as_ptr(),
                rule_ptrs.len(),
                invariant_ptrs.as_ptr(),
                invariant_ptrs.len(),
                &mut machine_id,
            );
            assert_eq!(rc, HEGEL_OK, "new_state_machine failed: rc={}", rc);
            assert_eq!(machine_id, 0);

            let mut overran = false;
            let mut current_run = 0usize;
            let mut previous: Option<u64> = None;
            for _ in 0..25 {
                let mut index: u64 = u64::MAX;
                let rc = (a.state_machine_next_rule)(ctx, tc, machine_id, &mut index);
                if rc == HEGEL_E_STOP_TEST {
                    overran = true;
                    break;
                }
                assert_eq!(rc, HEGEL_OK, "state_machine_next_rule failed: rc={}", rc);
                assert!(index < 3, "rule index {} out of range", index);
                saw_rule_draw = true;
                current_run = if previous == Some(index) {
                    current_run + 1
                } else {
                    1
                };
                previous = Some(index);
                longest_single_rule_run = longest_single_rule_run.max(current_run);
            }

            if overran {
                a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
            } else {
                a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
            }
        }

        assert!(saw_rule_draw, "expected at least one rule selection");
        assert!(
            longest_single_rule_run >= 15,
            "expected a long single-rule run under swarm selection, longest was {}",
            longest_single_rule_run
        );

        let result = a.run_result(ctx, run);
        assert!(!result.is_null());
        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Passed,
            "expected to pass"
        );
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn libhegel_primitive_boolean_draws_and_forces() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 50);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 11, true);

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());

        let mut saw_true = false;
        let mut saw_false = false;
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
                assert_eq!(err, "", "next_test_case returned NULL with error: {}", err);
                break;
            }

            let mut v = false;
            let rc = (a.primitive_boolean)(ctx, tc, 0.5, true, true, &mut v);
            assert_eq!(rc, HEGEL_OK, "forced-true draw failed: rc={}", rc);
            assert!(v);
            let rc = (a.primitive_boolean)(ctx, tc, 0.5, false, true, &mut v);
            assert_eq!(rc, HEGEL_OK, "forced-false draw failed: rc={}", rc);
            assert!(!v);

            let rc = (a.primitive_boolean)(ctx, tc, 0.0, false, false, &mut v);
            assert_eq!(rc, HEGEL_OK, "p=0 draw failed: rc={}", rc);
            assert!(!v);
            let rc = (a.primitive_boolean)(ctx, tc, 1.0, false, false, &mut v);
            assert_eq!(rc, HEGEL_OK, "p=1 draw failed: rc={}", rc);
            assert!(v);

            let rc = (a.primitive_boolean)(ctx, tc, 0.5, false, false, &mut v);
            if rc == HEGEL_E_STOP_TEST {
                a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, HEGEL_OK, "unforced draw failed: rc={}", rc);
            if v {
                saw_true = true;
            } else {
                saw_false = true;
            }

            a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        }

        assert!(saw_true, "expected an unforced draw to come up true");
        assert!(saw_false, "expected an unforced draw to come up false");

        let result = a.run_result(ctx, run);
        assert!(!result.is_null());
        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Passed,
            "expected a passed run"
        );
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn libhegel_primitive_boolean_rejects_invalid_arguments() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 1);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());

        let run = a.run_start(ctx, s);
        assert!(!run.is_null());

        let mut saw_test_case = false;
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                break;
            }
            saw_test_case = true;

            let mut v = false;
            let rc = (a.primitive_boolean)(ctx, ptr::null_mut(), 0.5, false, false, &mut v);
            assert_eq!(
                rc, HEGEL_E_INVALID_HANDLE,
                "expected HEGEL_E_INVALID_HANDLE, got rc={}",
                rc
            );

            let invalid: [(f64, bool, bool); 5] = [
                (f64::NAN, false, false),
                (-0.5, false, false),
                (1.5, false, false),
                (0.0, true, true),
                (1.0, false, true),
            ];
            for (p, forced, has_forced) in invalid {
                let rc = (a.primitive_boolean)(ctx, tc, p, forced, has_forced, &mut v);
                assert_eq!(
                    rc, HEGEL_E_INVALID_ARG,
                    "expected HEGEL_E_INVALID_ARG for p={}, forced={}, has_forced={}",
                    p, forced, has_forced
                );
                let err = CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy();
                assert!(!err.is_empty(), "expected a diagnostic message");
            }

            let rc = (a.primitive_boolean)(ctx, tc, 0.5, false, false, ptr::null_mut());
            assert_eq!(
                rc, HEGEL_E_INVALID_ARG,
                "expected HEGEL_E_INVALID_ARG for null out"
            );

            let rc = (a.primitive_boolean)(ctx, tc, 0.5, false, false, &mut v);
            assert_eq!(
                rc, HEGEL_OK,
                "valid draw after rejections failed: rc={}",
                rc
            );

            a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        }
        assert!(saw_test_case, "expected the run to produce a test case");

        let result = a.run_result(ctx, run);
        assert!(!result.is_null());
        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Passed,
            "expected a passed run"
        );
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn next_test_case_without_mark_complete_errors() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 5);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 7, true);

        let run = a.run_start(ctx, s);
        let tc1 = a.next_test_case(ctx, run);
        assert!(!tc1.is_null());
        let mut tc2: *mut u8 = ptr::null_mut();
        let rc = (a.next_test_case)(ctx, run, &mut tc2);
        assert_eq!(rc, HEGEL_E_NOT_COMPLETE, "expected NOT_COMPLETE");
        assert!(tc2.is_null(), "expected NULL on second next_test_case");
        let err = CStr::from_ptr(a.context_last_error(ctx))
            .to_string_lossy()
            .into_owned();
        assert!(err.contains("not marked complete"), "got: {}", err);

        a.complete_and_free(ctx, tc1, CStatus::Valid, ptr::null());
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                break;
            }
            a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
        }

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
}

#[test]
fn run_free_after_early_exit_does_not_hang() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 100);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 99, true);

        let run = a.run_start(ctx, s);
        let tc = a.next_test_case(ctx, run);
        a.run_free(ctx, run);
        // The caller still owns its handle after an early exit; free it now (as
        // a GC finaliser would), which drops the family's last reference.
        (a.test_case_free)(ctx, tc);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
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

    let mut last_failure: Option<i128> = None;
    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 200);
        (a.settings_database)(ctx, s, db_path.as_ptr());
        (a.settings_database_key)(ctx, s, key.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 1, true);

        let run = a.run_start(ctx, s);
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                ctx,
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            if rc == HEGEL_E_STOP_TEST {
                a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, HEGEL_OK);
            let v = decode(std::slice::from_raw_parts(val_ptr, val_len));
            let Value::Integer(i) = v else {
                panic!("expected integer")
            };
            let n: i128 = i.into();
            if predicate(n) {
                last_failure = Some(n);
                let origin = CString::new("n >= 1_000_000").unwrap();
                a.complete_and_free(ctx, tc, CStatus::Interesting, origin.as_ptr());
            } else {
                a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
            }
        }
        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }
    assert!(last_failure.is_some(), "run 1 never observed the failure");

    let mut first_seen: Option<i128> = None;
    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 200);
        (a.settings_database)(ctx, s, db_path.as_ptr());
        (a.settings_database_key)(ctx, s, key.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 1, true);

        let run = a.run_start(ctx, s);
        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                ctx,
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            if rc == HEGEL_E_STOP_TEST {
                a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
                continue;
            }
            assert_eq!(rc, HEGEL_OK);
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
                a.complete_and_free(ctx, tc, CStatus::Interesting, origin.as_ptr());
            } else {
                a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
            }
        }
        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
    }

    let first = first_seen.expect("run 2 never received a test case");
    assert!(
        predicate(first),
        "expected replay of n>=1_000_000 as first test case, got n={}",
        first
    );
}

#[test]
fn health_check_surfaces_as_run_error() {
    let lib = unsafe { load() };
    let a = unsafe { bind(&lib) };

    unsafe {
        let ctx = (a.context_new)();
        let s = a.settings_new(ctx);
        a.settings_test_cases(ctx, s, 200);
        let empty = CString::new("").unwrap();
        (a.settings_database)(ctx, s, empty.as_ptr());
        a.settings_derandomize(ctx, s, true);
        a.settings_seed(ctx, s, 1, true);

        let run = a.run_start(ctx, s);
        let schema = integer_schema(0, 1_000_000);

        loop {
            let tc = a.next_test_case(ctx, run);
            if tc.is_null() {
                break;
            }
            let mut val_ptr: *const u8 = ptr::null();
            let mut val_len: usize = 0;
            let rc = (a.generate)(
                ctx,
                tc,
                schema.as_ptr(),
                schema.len(),
                &mut val_ptr,
                &mut val_len,
            );
            let _ = (val_ptr, val_len);
            if rc == HEGEL_E_STOP_TEST {
                a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
            } else {
                a.complete_and_free(ctx, tc, CStatus::Invalid, ptr::null());
            }
        }

        let last_err = CStr::from_ptr(a.context_last_error(ctx))
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            last_err, "",
            "next_test_case loop ended with error instead of normal completion: {}",
            last_err
        );

        let result = a.run_result(ctx, run);
        assert!(
            !result.is_null(),
            "hegel_run_result returned NULL after the health check fired; \
             last_error = {}",
            CStr::from_ptr(a.context_last_error(ctx)).to_string_lossy()
        );

        assert_eq!(
            a.run_result_status(ctx, result),
            CRunStatus::Error,
            "expected an errored run after the health check fired"
        );
        assert_eq!(
            a.run_result_failure_count(ctx, result),
            0,
            "a run-level error is not a failure of any test case"
        );
        let err_ptr = a.run_result_error(ctx, result);
        assert!(
            !err_ptr.is_null(),
            "expected hegel_run_result_error to carry the health-check message"
        );
        let msg = CStr::from_ptr(err_ptr).to_string_lossy();
        assert!(
            msg.contains("FilterTooMuch"),
            "expected the run error to reference FilterTooMuch, got: {}",
            msg
        );
        a.run_result_free(ctx, result);

        a.run_free(ctx, run);
        a.settings_free(ctx, s);
        (a.context_free)(ctx);
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
            let ctx = (a.context_new)();
            let s = a.settings_new(ctx);
            a.settings_test_cases(ctx, s, 100);
            (a.settings_database)(ctx, s, empty.as_ptr());
            a.settings_derandomize(ctx, s, true);
            a.settings_seed(ctx, s, seed, true);

            let run = a.run_start(ctx, s);
            loop {
                let tc = a.next_test_case(ctx, run);
                if tc.is_null() {
                    break;
                }
                let mut val_ptr: *const u8 = ptr::null();
                let mut val_len: usize = 0;
                let rc = (a.generate)(
                    ctx,
                    tc,
                    schema.as_ptr(),
                    schema.len(),
                    &mut val_ptr,
                    &mut val_len,
                );
                if rc == HEGEL_E_STOP_TEST {
                    a.complete_and_free(ctx, tc, CStatus::Overrun, ptr::null());
                    continue;
                }
                assert_eq!(rc, HEGEL_OK);
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
                    a.complete_and_free(ctx, tc, CStatus::Interesting, origin_ptr);
                } else {
                    a.complete_and_free(ctx, tc, CStatus::Valid, ptr::null());
                }
            }
            a.run_free(ctx, run);
            a.settings_free(ctx, s);
            (a.context_free)(ctx);
        }
        let final_value = last_failing.unwrap_or(i128::MIN);
        observed.push(final_value);
        if final_value == 1_000_000 {
            hits += 1;
        }
    }
    (hits, observed)
}
