// C shared library bindings for Hegel's native property-based testing engine.
//
// The public C surface is documented in `include/hegel.h` (generated from
// this file by cbindgen). Architectural overview:
//
// - Each `hegel_run_start` spawns a worker thread that drives
//   `hegeltest::embed::run_native`. The worker is isolated from the caller
//   so that any foreign unwinding (longjmp / C++ exception / LuaJIT error)
//   from the C side only damages the caller's stack — the engine's stack
//   is untouched.
//
// - The worker and caller communicate via a channel. For each test case
//   the engine wants to run, the worker sends the raw `DataSource` to the
//   caller and blocks waiting for an ack. The caller reaches in, runs its
//   test logic on the data source directly, calls `mark_complete`, and
//   sends the ack. The hot path (`generate`, spans, etc.) calls the
//   `DataSource` methods directly without channel traffic — `DataSource`
//   is `Send + Sync` so once handed across it works in place.
//
// - Errors are signalled via int return codes (`HEGEL_E_*`) on per-test-case
//   primitives, or NULL returns with a thread-local last_error string for
//   handle-level calls. There is no callback into C from Rust on the hot
//   path; the loop is user-driven.

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_int};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use ciborium::Value;
use hegel::backend::{DataSource, DataSourceError, Failure, TestCaseResult, TestRunResult};
use hegel::embed::run_native;
use hegel::{HealthCheck, Mode, Phase, Settings, Verbosity};

// ─── Error codes ────────────────────────────────────────────────────────────

pub const HEGEL_OK: c_int = 0;
pub const HEGEL_E_STOP_TEST: c_int = -1;
pub const HEGEL_E_ASSUME: c_int = -2;
pub const HEGEL_E_BACKEND: c_int = -3;
pub const HEGEL_E_INVALID_HANDLE: c_int = -4;
pub const HEGEL_E_INVALID_ARG: c_int = -5;
pub const HEGEL_E_ALREADY_COMPLETE: c_int = -6;
pub const HEGEL_E_NOT_COMPLETE: c_int = -7;
pub const HEGEL_E_INTERNAL: c_int = -8;

// ─── Enums mirrored to C ────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
pub enum HegelStatus {
    Valid = 0,
    Invalid = 1,
    Overrun = 2,
    Interesting = 3,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum HegelMode {
    TestRun = 0,
    SingleTestCase = 1,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum HegelVerbosity {
    Quiet = 0,
    Normal = 1,
    Verbose = 2,
    Debug = 3,
}

// Bitmask constants for phases / health checks. Exported via cbindgen as
// `pub const`s alongside the structs.

pub const HEGEL_PHASE_EXPLICIT: u32 = 1 << 0;
pub const HEGEL_PHASE_REUSE: u32 = 1 << 1;
pub const HEGEL_PHASE_GENERATE: u32 = 1 << 2;
pub const HEGEL_PHASE_TARGET: u32 = 1 << 3;
pub const HEGEL_PHASE_SHRINK: u32 = 1 << 4;
pub const HEGEL_PHASE_ALL: u32 = 0x1F;

pub const HEGEL_HC_FILTER_TOO_MUCH: u32 = 1 << 0;
pub const HEGEL_HC_TOO_SLOW: u32 = 1 << 1;
pub const HEGEL_HC_TEST_CASES_TOO_LARGE: u32 = 1 << 2;
pub const HEGEL_HC_LARGE_INITIAL_TEST_CASE: u32 = 1 << 3;

// Span labels — mirror hegeltest::test_case::labels.
pub const HEGEL_LABEL_LIST: u64 = 1;
pub const HEGEL_LABEL_LIST_ELEMENT: u64 = 2;
pub const HEGEL_LABEL_SET: u64 = 3;
pub const HEGEL_LABEL_SET_ELEMENT: u64 = 4;
pub const HEGEL_LABEL_MAP: u64 = 5;
pub const HEGEL_LABEL_MAP_ENTRY: u64 = 6;
pub const HEGEL_LABEL_TUPLE: u64 = 7;
pub const HEGEL_LABEL_ONE_OF: u64 = 8;
pub const HEGEL_LABEL_OPTIONAL: u64 = 9;
pub const HEGEL_LABEL_FIXED_DICT: u64 = 10;
pub const HEGEL_LABEL_FLAT_MAP: u64 = 11;
pub const HEGEL_LABEL_FILTER: u64 = 12;
pub const HEGEL_LABEL_MAPPED: u64 = 13;
pub const HEGEL_LABEL_SAMPLED_FROM: u64 = 14;
pub const HEGEL_LABEL_ENUM_VARIANT: u64 = 15;

// ─── Thread-local error message ─────────────────────────────────────────────

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::new("").unwrap());
}

fn set_last_error(msg: &str) {
    let c =
        CString::new(msg).unwrap_or_else(|_| CString::new("error message contained NUL").unwrap());
    LAST_ERROR.with(|cell| *cell.borrow_mut() = c);
}

fn clear_last_error() {
    LAST_ERROR.with(|cell| *cell.borrow_mut() = CString::new("").unwrap());
}

// ─── HegelSettings ──────────────────────────────────────────────────────────

pub struct HegelSettings {
    inner: Settings,
    /// Optional database key used by the runner for example storage / replay.
    /// Not part of `Settings` itself in upstream hegel; passed as a separate
    /// argument to `run_native` on `hegel_run_start`.
    database_key: Option<String>,
}

// ─── HegelRun / HegelTestCase / channel plumbing ────────────────────────────

enum WorkerMessage {
    TestCase {
        ds: Box<dyn DataSource + Send + Sync>,
        is_final: bool,
        ack: mpsc::Sender<()>,
    },
    Done(TestRunResult),
}

pub struct HegelTestCase {
    ds: Box<dyn DataSource + Send + Sync>,
    is_final: bool,
    completed: bool,
    /// Backing buffer for the borrowed `out_value_cbor` pointer returned
    /// from `hegel_generate`. Re-allocated per call; the previous draw's
    /// bytes are invalidated on the next `hegel_generate`.
    last_value: Vec<u8>,
    ack: mpsc::Sender<()>,
}

pub struct HegelRun {
    worker: Option<JoinHandle<()>>,
    from_worker: mpsc::Receiver<WorkerMessage>,
    abort: Arc<AtomicBool>,
    current_tc: Option<Box<HegelTestCase>>,
    result: Option<HegelRunResult>,
    /// Set once a TestRunDone (or worker-died Err) has been observed on
    /// `from_worker`. Stops `hegel_next_test_case` from blocking forever
    /// on the second post-completion call.
    drained: bool,
}

pub struct HegelRunResult {
    passed: bool,
    failures: Vec<HegelFailure>,
}

pub struct HegelFailure {
    panic_message: CString,
    diagnostic: CString,
    origin: CString,
}

impl From<Failure> for HegelFailure {
    fn from(f: Failure) -> Self {
        HegelFailure {
            panic_message: cstring_lossy(&f.panic_message),
            diagnostic: cstring_lossy(&f.diagnostic),
            origin: cstring_lossy(&f.origin),
        }
    }
}

impl From<TestRunResult> for HegelRunResult {
    fn from(r: TestRunResult) -> Self {
        HegelRunResult {
            passed: r.passed,
            failures: r.failures.into_iter().map(HegelFailure::from).collect(),
        }
    }
}

/// Replace interior NULs (which can't appear in C strings) with the
/// REPLACEMENT CHARACTER's underline. Hegel-produced diagnostic strings
/// shouldn't contain NULs, but defending against that here means the
/// caller never sees `CString::new` panic.
fn cstring_lossy(s: &str) -> CString {
    let sanitized: String = s
        .chars()
        .map(|c| if c == '\0' { '\u{FFFD}' } else { c })
        .collect();
    CString::new(sanitized).expect("NULs replaced above")
}

// ─── Settings extern functions ──────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn hegel_settings_new() -> *mut HegelSettings {
    Box::into_raw(Box::new(HegelSettings {
        inner: Settings::new(),
        database_key: None,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_free(s: *mut HegelSettings) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) });
    }
}

unsafe fn settings_mut<'a>(s: *mut HegelSettings) -> Option<&'a mut Settings> {
    unsafe { s.as_mut() }.map(|h| &mut h.inner)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_mode(s: *mut HegelSettings, mode: HegelMode) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        let m = match mode {
            HegelMode::TestRun => Mode::TestRun,
            HegelMode::SingleTestCase => Mode::SingleTestCase,
        };
        *inner = inner.clone().mode(m);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_test_cases(s: *mut HegelSettings, n: u64) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().test_cases(n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_verbosity(s: *mut HegelSettings, v: HegelVerbosity) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        let verbosity = match v {
            HegelVerbosity::Quiet => Verbosity::Quiet,
            HegelVerbosity::Normal => Verbosity::Normal,
            HegelVerbosity::Verbose => Verbosity::Verbose,
            HegelVerbosity::Debug => Verbosity::Debug,
        };
        *inner = inner.clone().verbosity(verbosity);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_seed(s: *mut HegelSettings, seed: u64, has_seed: bool) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().seed(if has_seed { Some(seed) } else { None });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_derandomize(s: *mut HegelSettings, derandomize: bool) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().derandomize(derandomize);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_report_multiple_failures(s: *mut HegelSettings, yes: bool) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().report_multiple_failures(yes);
    }
}

/// `database = NULL` → default; `database = ""` → disabled; else → path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_database(s: *mut HegelSettings, database: *const c_char) {
    let Some(inner) = (unsafe { settings_mut(s) }) else {
        return;
    };
    if database.is_null() {
        // "Use default" — currently same as leaving Settings::new()'s default.
        return;
    }
    let cstr = unsafe { CStr::from_ptr(database) };
    match cstr.to_str() {
        Ok("") => *inner = inner.clone().database(None),
        Ok(path) => *inner = inner.clone().database(Some(path.to_string())),
        Err(_) => set_last_error("hegel_settings_database: path is not valid UTF-8"),
    }
}

/// Set the database key used to scope stored / replayed examples for this run.
/// `key = NULL` clears it (the default).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_database_key(
    s: *mut HegelSettings,
    key: *const c_char,
) {
    let Some(hs) = (unsafe { s.as_mut() }) else { return };
    if key.is_null() {
        hs.database_key = None;
        return;
    }
    match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(k) => hs.database_key = Some(k.to_string()),
        Err(_) => set_last_error("hegel_settings_database_key: key is not valid UTF-8"),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_phases(s: *mut HegelSettings, phases: u32) {
    let Some(inner) = (unsafe { settings_mut(s) }) else {
        return;
    };
    let mut v = Vec::new();
    if phases & HEGEL_PHASE_EXPLICIT != 0 {
        v.push(Phase::Explicit);
    }
    if phases & HEGEL_PHASE_REUSE != 0 {
        v.push(Phase::Reuse);
    }
    if phases & HEGEL_PHASE_GENERATE != 0 {
        v.push(Phase::Generate);
    }
    if phases & HEGEL_PHASE_TARGET != 0 {
        v.push(Phase::Target);
    }
    if phases & HEGEL_PHASE_SHRINK != 0 {
        v.push(Phase::Shrink);
    }
    *inner = inner.clone().phases(v);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_suppress_health_check(s: *mut HegelSettings, checks: u32) {
    let Some(inner) = (unsafe { settings_mut(s) }) else {
        return;
    };
    let mut v = Vec::new();
    if checks & HEGEL_HC_FILTER_TOO_MUCH != 0 {
        v.push(HealthCheck::FilterTooMuch);
    }
    if checks & HEGEL_HC_TOO_SLOW != 0 {
        v.push(HealthCheck::TooSlow);
    }
    if checks & HEGEL_HC_TEST_CASES_TOO_LARGE != 0 {
        v.push(HealthCheck::TestCasesTooLarge);
    }
    if checks & HEGEL_HC_LARGE_INITIAL_TEST_CASE != 0 {
        v.push(HealthCheck::LargeInitialTestCase);
    }
    *inner = inner.clone().suppress_health_check(v);
}

// ─── Run lifecycle ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_start(settings: *const HegelSettings) -> *mut HegelRun {
    clear_last_error();
    let Some(handle) = (unsafe { settings.as_ref() }) else {
        set_last_error("hegel_run_start: settings pointer is null");
        return ptr::null_mut();
    };
    let settings = handle.inner.clone();
    let database_key = handle.database_key.clone();

    let (to_caller, from_worker) = mpsc::channel::<WorkerMessage>();
    let abort = Arc::new(AtomicBool::new(false));
    let abort_worker = Arc::clone(&abort);

    let worker = thread::Builder::new()
        .name("hegel-worker".to_string())
        .spawn(move || {
            let result = run_native(&settings, database_key.as_deref(), |ds, is_final| {
                if abort_worker.load(Ordering::Acquire) {
                    ds.mark_complete(&TestCaseResult::Valid);
                    return;
                }
                let (ack_tx, ack_rx) = mpsc::channel();
                let msg = WorkerMessage::TestCase {
                    ds,
                    is_final,
                    ack: ack_tx,
                };
                if let Err(mpsc::SendError(returned)) = to_caller.send(msg) {
                    // Caller dropped — recover the data source we just tried
                    // to hand off and mark it complete so the engine can
                    // make progress to its (now-irrelevant) end.
                    if let WorkerMessage::TestCase { ds, .. } = returned {
                        ds.mark_complete(&TestCaseResult::Valid);
                    }
                    return;
                }
                // Caller dropping the ack sender is treated the same as
                // a successful ack — we're winding down regardless.
                let _ = ack_rx.recv();
            });
            let _ = to_caller.send(WorkerMessage::Done(result));
        });

    let worker = match worker {
        Ok(h) => h,
        Err(e) => {
            set_last_error(&format!("hegel_run_start: failed to spawn worker: {}", e));
            return ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(HegelRun {
        worker: Some(worker),
        from_worker,
        abort,
        current_tc: None,
        result: None,
        drained: false,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_next_test_case(run: *mut HegelRun) -> *mut HegelTestCase {
    clear_last_error();
    let Some(run) = (unsafe { run.as_mut() }) else {
        set_last_error("hegel_next_test_case: run pointer is null");
        return ptr::null_mut();
    };

    // The previous test case must have been completed.
    if let Some(tc) = run.current_tc.as_ref() {
        if !tc.completed {
            set_last_error(
                "hegel_next_test_case: previous test case was not marked complete \
                 (call hegel_mark_complete before requesting the next case)",
            );
            return ptr::null_mut();
        }
    }
    run.current_tc = None;

    if run.drained {
        // Run already completed; calling next again returns NULL with no error.
        return ptr::null_mut();
    }

    match run.from_worker.recv() {
        Ok(WorkerMessage::TestCase { ds, is_final, ack }) => {
            let tc = Box::new(HegelTestCase {
                ds,
                is_final,
                completed: false,
                last_value: Vec::new(),
                ack,
            });
            let ptr = (&*tc) as *const HegelTestCase as *mut HegelTestCase;
            run.current_tc = Some(tc);
            ptr
        }
        Ok(WorkerMessage::Done(r)) => {
            run.result = Some(HegelRunResult::from(r));
            run.drained = true;
            ptr::null_mut()
        }
        Err(_) => {
            // Worker dropped its sender without sending Done — should not
            // happen in normal use, but treat as a soft EOF rather than
            // panicking. Caller distinguishes via last_error.
            run.drained = true;
            set_last_error("hegel_next_test_case: worker terminated without reporting a result");
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result(run: *mut HegelRun) -> *const HegelRunResult {
    clear_last_error();
    let Some(run) = (unsafe { run.as_ref() }) else {
        set_last_error("hegel_run_result: run pointer is null");
        return ptr::null();
    };
    match &run.result {
        Some(r) => r as *const HegelRunResult,
        None => {
            set_last_error("hegel_run_result: run has not finished yet");
            ptr::null()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_free(run: *mut HegelRun) {
    if run.is_null() {
        return;
    }
    let mut run = unsafe { Box::from_raw(run) };

    // Signal the worker to short-circuit any further test cases.
    run.abort.store(true, Ordering::Release);

    // If a test case is still in flight (caller exited the loop early),
    // complete it so the worker's wait-for-ack unblocks.
    if let Some(mut tc) = run.current_tc.take() {
        if !tc.completed {
            tc.ds.mark_complete(&TestCaseResult::Valid);
            let _ = tc.ack.send(());
            tc.completed = true;
        }
    }

    // Drain anything else the worker emits before it finishes winding down.
    // After the abort flag, the worker's callback short-circuits without
    // sending, so this typically receives just the final Done message and
    // then the channel closes.
    while let Ok(msg) = run.from_worker.recv() {
        if let WorkerMessage::TestCase { ds, ack, .. } = msg {
            ds.mark_complete(&TestCaseResult::Valid);
            let _ = ack.send(());
        }
    }

    if let Some(handle) = run.worker.take() {
        let _ = handle.join();
    }
}

// ─── Per-test-case primitives ───────────────────────────────────────────────

unsafe fn tc_mut<'a>(tc: *mut HegelTestCase) -> Result<&'a mut HegelTestCase, c_int> {
    let tc = unsafe { tc.as_mut() }.ok_or(HEGEL_E_INVALID_HANDLE)?;
    if tc.completed {
        return Err(HEGEL_E_ALREADY_COMPLETE);
    }
    Ok(tc)
}

fn translate_ds_error(e: DataSourceError) -> c_int {
    match e {
        DataSourceError::StopTest => HEGEL_E_STOP_TEST,
        DataSourceError::Assume => HEGEL_E_ASSUME,
        DataSourceError::ServerError(msg) => {
            set_last_error(&msg);
            HEGEL_E_BACKEND
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate(
    tc: *mut HegelTestCase,
    schema_cbor: *const u8,
    schema_len: usize,
    out_value_cbor: *mut *const u8,
    out_value_len: *mut usize,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if schema_cbor.is_null() && schema_len > 0 {
        set_last_error("hegel_generate: schema pointer is null");
        return HEGEL_E_INVALID_ARG;
    }
    if out_value_cbor.is_null() || out_value_len.is_null() {
        set_last_error("hegel_generate: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }

    let schema_bytes = unsafe { std::slice::from_raw_parts(schema_cbor, schema_len) };
    let schema: Value = match ciborium::de::from_reader(schema_bytes) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(&format!("hegel_generate: malformed CBOR schema: {}", e));
            return HEGEL_E_INVALID_ARG;
        }
    };

    let value = match tc.ds.generate(&schema) {
        Ok(v) => v,
        Err(e) => return translate_ds_error(e),
    };

    tc.last_value.clear();
    if let Err(e) = ciborium::ser::into_writer(&value, &mut tc.last_value) {
        set_last_error(&format!(
            "hegel_generate: failed to re-serialize value: {}",
            e
        ));
        return HEGEL_E_INTERNAL;
    }
    unsafe {
        *out_value_cbor = tc.last_value.as_ptr();
        *out_value_len = tc.last_value.len();
    }
    HEGEL_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_start_span(tc: *mut HegelTestCase, label: u64) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.ds.start_span(label) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(e),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_stop_span(tc: *mut HegelTestCase, discard: bool) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.ds.stop_span(discard) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(e),
    }
}

/// `max_size = UINT64_MAX` (i.e. `u64::MAX`) means unbounded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_collection(
    tc: *mut HegelTestCase,
    min_size: u64,
    max_size: u64,
    out_collection_id: *mut i64,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_collection_id.is_null() {
        set_last_error("hegel_new_collection: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let max = if max_size == u64::MAX {
        None
    } else {
        Some(max_size)
    };
    match tc.ds.new_collection(min_size, max) {
        Ok(id) => {
            unsafe { *out_collection_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(e),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_more(
    tc: *mut HegelTestCase,
    collection_id: i64,
    out_more: *mut bool,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_more.is_null() {
        set_last_error("hegel_collection_more: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.ds.collection_more(collection_id) {
        Ok(m) => {
            unsafe { *out_more = m };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(e),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_reject(
    tc: *mut HegelTestCase,
    collection_id: i64,
    why: *const c_char,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let why_str = if why.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(why) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => {
                set_last_error("hegel_collection_reject: why is not valid UTF-8");
                return HEGEL_E_INVALID_ARG;
            }
        }
    };
    match tc.ds.collection_reject(collection_id, why_str) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(e),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_target(
    tc: *mut HegelTestCase,
    value: f64,
    label: *const c_char,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if label.is_null() {
        set_last_error("hegel_target: label is null");
        return HEGEL_E_INVALID_ARG;
    }
    let label = match unsafe { CStr::from_ptr(label) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("hegel_target: label is not valid UTF-8");
            return HEGEL_E_INVALID_ARG;
        }
    };
    tc.ds.target_observation(value, label);
    HEGEL_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_mark_complete(
    tc: *mut HegelTestCase,
    status: HegelStatus,
    origin: *const c_char,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc.as_mut() } {
        Some(t) => t,
        None => return HEGEL_E_INVALID_HANDLE,
    };
    if tc.completed {
        return HEGEL_E_ALREADY_COMPLETE;
    }

    let outcome = match status {
        HegelStatus::Valid => TestCaseResult::Valid,
        HegelStatus::Invalid => TestCaseResult::Invalid,
        HegelStatus::Overrun => TestCaseResult::Overrun,
        HegelStatus::Interesting => {
            let origin_str = if origin.is_null() {
                "Panic at <unknown>".to_string()
            } else {
                match unsafe { CStr::from_ptr(origin) }.to_str() {
                    Ok(s) => s.to_string(),
                    Err(_) => {
                        set_last_error("hegel_mark_complete: origin is not valid UTF-8");
                        return HEGEL_E_INVALID_ARG;
                    }
                }
            };
            TestCaseResult::Interesting(Failure {
                panic_message: origin_str.clone(),
                diagnostic: format!("Failure reported by C caller: {}\n", origin_str),
                origin: origin_str,
            })
        }
    };

    tc.ds.mark_complete(&outcome);
    let _ = tc.ack.send(());
    tc.completed = true;
    HEGEL_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_is_final_replay(tc: *const HegelTestCase) -> bool {
    match unsafe { tc.as_ref() } {
        Some(t) => t.is_final,
        None => false,
    }
}

// ─── Result inspection ──────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_passed(r: *const HegelRunResult) -> bool {
    match unsafe { r.as_ref() } {
        Some(r) => r.passed,
        None => false,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_failure_count(r: *const HegelRunResult) -> usize {
    match unsafe { r.as_ref() } {
        Some(r) => r.failures.len(),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_failure(
    r: *const HegelRunResult,
    index: usize,
) -> *const HegelFailure {
    match unsafe { r.as_ref() } {
        Some(r) => r
            .failures
            .get(index)
            .map(|f| f as *const HegelFailure)
            .unwrap_or(ptr::null()),
        None => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_panic_message(f: *const HegelFailure) -> *const c_char {
    match unsafe { f.as_ref() } {
        Some(f) => f.panic_message.as_ptr(),
        None => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_diagnostic(f: *const HegelFailure) -> *const c_char {
    match unsafe { f.as_ref() } {
        Some(f) => f.diagnostic.as_ptr(),
        None => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_origin(f: *const HegelFailure) -> *const c_char {
    match unsafe { f.as_ref() } {
        Some(f) => f.origin.as_ptr(),
        None => ptr::null(),
    }
}

// ─── Diagnostics ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn hegel_last_error_message() -> *const c_char {
    LAST_ERROR.with(|cell| {
        // Returning a borrowed pointer into a thread-local RefCell is sound
        // as long as no other libhegel call on this thread fires before the
        // caller is done with the pointer. The C header documents that.
        let r = cell.borrow();
        r.as_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn hegel_version() -> *const c_char {
    // Static CStr in the binary; pointer is valid for the program lifetime.
    static VERSION: &CStr =
        match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
            Ok(c) => c,
            Err(_) => unreachable!(),
        };
    VERSION.as_ptr()
}
