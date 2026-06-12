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
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use ciborium::Value;
use hegel::backend::{DataSource, DataSourceError, Failure, TestCaseResult, TestRunResult};
use hegel::embed::{data_source_for_blob, run_native};
use hegel::{HealthCheck, Mode, Phase, Settings, Verbosity};

// ─── Error codes ────────────────────────────────────────────────────────────
//
// All `int`-returning entry points (the per-test-case primitives, etc.)
// return one of these. Handle-returning entry points use NULL instead and
// leave a description in `hegel_last_error_message()`.

/// Success.
pub const HEGEL_OK: c_int = 0;

/// The engine has exhausted its choice budget for this test case and
/// wants the caller to abort the body and return. Treat the same as a
/// validly-completed test case.
pub const HEGEL_E_STOP_TEST: c_int = -1;

/// An `assume` / `reject` precondition failed. The current test case is
/// invalid and should be discarded.
pub const HEGEL_E_ASSUME: c_int = -2;

/// The underlying engine reported an error. See
/// `hegel_last_error_message()` for the diagnostic.
pub const HEGEL_E_BACKEND: c_int = -3;

/// A handle pointer (`hegel_settings_t*`, `hegel_run_t*`,
/// `hegel_test_case_t*`, …) was NULL where it must be non-NULL.
pub const HEGEL_E_INVALID_HANDLE: c_int = -4;

/// An argument other than a handle was invalid — NULL where a value was
/// required, malformed CBOR, non-UTF-8 string, etc. See
/// `hegel_last_error_message()` for specifics.
pub const HEGEL_E_INVALID_ARG: c_int = -5;

/// `hegel_mark_complete` (or a primitive on the same handle) was called
/// for a test case that has already been completed.
pub const HEGEL_E_ALREADY_COMPLETE: c_int = -6;

/// `hegel_next_test_case` was called without first completing the
/// previous test case with `hegel_mark_complete`.
pub const HEGEL_E_NOT_COMPLETE: c_int = -7;

/// An internal invariant failed inside libhegel (e.g. CBOR
/// re-serialisation). Should not happen in practice; please file a
/// bug. See `hegel_last_error_message()` for the diagnostic.
pub const HEGEL_E_INTERNAL: c_int = -8;

// ─── Enums mirrored to C ────────────────────────────────────────────────────

// The enum types and variants use C naming directly. Rust complains about
// the conventions (non_camel_case_types for the type, non_snake_case isn't
// the right lint here — it's that variants aren't camelCase), so we silence
// the lint. The payoff is that cbindgen produces clean idiomatic C:
//
//   typedef enum {
//       HEGEL_STATUS_VALID = 0,
//       ...
//   } hegel_status_t;
//
// without the HEGEL_STATUS_T_VALID-style mangling we'd get from cbindgen's
// `prefix_with_name`.

/// Outcome of a single test case. Passed to `hegel_mark_complete`.
///
/// - `HEGEL_STATUS_VALID`: the test body ran to completion without
///   finding an interesting outcome (the property held).
/// - `HEGEL_STATUS_INVALID`: an `assume` / precondition rejected this
///   draw; the engine should discard it without counting it against
///   the test-cases budget.
/// - `HEGEL_STATUS_OVERRUN`: the engine ran out of choice budget mid
///   test case (typically because `hegel_generate` returned
///   `HEGEL_E_STOP_TEST`); treat the case as inconclusive.
/// - `HEGEL_STATUS_INTERESTING`: the property failed and this draw is
///   a candidate counterexample. Pass a stable origin string to
///   `hegel_mark_complete` so the shrinker can identify the bug.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_status_t {
    HEGEL_STATUS_VALID = 0,
    HEGEL_STATUS_INVALID = 1,
    HEGEL_STATUS_OVERRUN = 2,
    HEGEL_STATUS_INTERESTING = 3,
}

/// How the engine should treat the run: a full property-test loop or a
/// single test case.
///
/// - `HEGEL_MODE_TEST_RUN`: the engine drives a full
///   generate / shrink / replay loop until `max_examples` or the
///   choice tree is exhausted.
/// - `HEGEL_MODE_SINGLE_TEST_CASE`: the engine produces exactly one
///   test case and stops, with no shrinking. Useful for replaying a
///   stored counterexample or running an exploratory probe.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_mode_t {
    HEGEL_MODE_TEST_RUN = 0,
    HEGEL_MODE_SINGLE_TEST_CASE = 1,
}

/// Aggregate outcome of a finished run, read via `hegel_run_result_status`.
///
/// - `HEGEL_RUN_STATUS_PASSED`: the property held across every generated
///   test case.
/// - `HEGEL_RUN_STATUS_FAILED`: the property failed; inspect each distinct
///   counterexample via `hegel_run_result_failure_count` /
///   `hegel_run_result_failure`.
/// - `HEGEL_RUN_STATUS_ERROR`: the run itself failed — a failed health
///   check, a nondeterministic test, an engine panic — and produced no
///   verdict on the property. There are no failures to inspect; the
///   message is read via `hegel_run_result_error`.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum hegel_run_status_t {
    HEGEL_RUN_STATUS_PASSED = 0,
    HEGEL_RUN_STATUS_FAILED = 1,
    HEGEL_RUN_STATUS_ERROR = 2,
}

/// Verbosity of engine-emitted output (logs, per-case traces). Set via
/// `hegel_settings_verbosity`.
///
/// - `HEGEL_VERBOSITY_QUIET`: nothing besides the final result.
/// - `HEGEL_VERBOSITY_NORMAL`: a short summary line per run (default).
/// - `HEGEL_VERBOSITY_VERBOSE`: per-test-case progress, drawn values
///   for the final replay, panic diagnostics as they happen.
/// - `HEGEL_VERBOSITY_DEBUG`: as verbose, plus Hypothesis-style
///   shrinker trace output.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_verbosity_t {
    HEGEL_VERBOSITY_QUIET = 0,
    HEGEL_VERBOSITY_NORMAL = 1,
    HEGEL_VERBOSITY_VERBOSE = 2,
    HEGEL_VERBOSITY_DEBUG = 3,
}

// ─── Phase bitmask ──────────────────────────────────────────────────────────
//
// Bitmask passed to `hegel_settings_phases` to enable / disable
// individual phases of the property-test loop. The default
// (`HEGEL_PHASE_ALL`) is almost always what you want; turning a phase
// off is mainly useful for debugging or replay tooling.

/// Run hard-coded explicit examples (none today, reserved for future use).
pub const HEGEL_PHASE_EXPLICIT: u32 = 1 << 0;

/// Replay counterexamples persisted from previous runs (requires a
/// database path + `hegel_settings_database_key`).
pub const HEGEL_PHASE_REUSE: u32 = 1 << 1;

/// Randomly generate fresh test cases up to the `test_cases` budget.
pub const HEGEL_PHASE_GENERATE: u32 = 1 << 2;

/// Apply hill-climbing toward observed `hegel_target` scores between
/// generation rounds.
pub const HEGEL_PHASE_TARGET: u32 = 1 << 3;

/// Shrink discovered failing examples toward minimal counterexamples.
pub const HEGEL_PHASE_SHRINK: u32 = 1 << 4;

/// Convenience: all five phases enabled. This is the default.
pub const HEGEL_PHASE_ALL: u32 = 0x1F;

// ─── Health-check bitmask ───────────────────────────────────────────────────
//
// Bitmask passed to `hegel_settings_suppress_health_check` to *disable*
// individual health checks. The default is "all enabled"; suppress only
// when you understand why the check is firing and accept it.

/// Suppress: aborts the run if too many draws are rejected via
/// `assume` / `Invalid` (default threshold: 200 in a row with no valid
/// case).
pub const HEGEL_HC_FILTER_TOO_MUCH: u32 = 1 << 0;

/// Suppress: aborts the run if individual test cases take so long that
/// the overall run is impractical.
pub const HEGEL_HC_TOO_SLOW: u32 = 1 << 1;

/// Suppress: aborts the run if generated values are so large that
/// retaining them for shrinking is impractical.
pub const HEGEL_HC_TEST_CASES_TOO_LARGE: u32 = 1 << 2;

/// Suppress: warns if the first generated test case is already
/// disproportionately large.
pub const HEGEL_HC_LARGE_INITIAL_TEST_CASE: u32 = 1 << 3;

// ─── Span labels ────────────────────────────────────────────────────────────
//
// Identifiers passed to `hegel_start_span` so the shrinker knows what
// kind of compound structure is being assembled. Pick whichever label
// best describes the surrounding context; the engine uses these to
// choose appropriate shrink moves (e.g. shortening lists vs. simplifying
// individual list elements). Mirror `hegeltest::test_case::labels`.

/// Outer span around a list / sequence.
pub const HEGEL_LABEL_LIST: u64 = 1;

/// One element of a list.
pub const HEGEL_LABEL_LIST_ELEMENT: u64 = 2;

/// Outer span around a set (unordered, no duplicates).
pub const HEGEL_LABEL_SET: u64 = 3;

/// One element of a set.
pub const HEGEL_LABEL_SET_ELEMENT: u64 = 4;

/// Outer span around a map / dictionary.
pub const HEGEL_LABEL_MAP: u64 = 5;

/// One (key, value) entry of a map.
pub const HEGEL_LABEL_MAP_ENTRY: u64 = 6;

/// Outer span around a tuple / fixed-arity record.
pub const HEGEL_LABEL_TUPLE: u64 = 7;

/// Outer span around a `one_of` / disjunction; useful so the shrinker
/// can swap which branch is taken.
pub const HEGEL_LABEL_ONE_OF: u64 = 8;

/// Outer span around an `optional` (None vs Some(value)).
pub const HEGEL_LABEL_OPTIONAL: u64 = 9;

/// Outer span around a fixed-shape record (named fields known
/// statically).
pub const HEGEL_LABEL_FIXED_DICT: u64 = 10;

/// Outer span around a `flat_map` / monadic dependent draw.
pub const HEGEL_LABEL_FLAT_MAP: u64 = 11;

/// Outer span around a `filter` / rejection-sampling wrapper.
pub const HEGEL_LABEL_FILTER: u64 = 12;

/// Outer span around a `map` / pure transformation.
pub const HEGEL_LABEL_MAPPED: u64 = 13;

/// Outer span around a `sampled_from` / pick-from-collection draw.
pub const HEGEL_LABEL_SAMPLED_FROM: u64 = 14;

/// Outer span around the variant discriminator of a sum-type draw.
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

/// Settings handle for a libhegel run.
///
/// Construct with `hegel_settings_new`, configure via the
/// `hegel_settings_*` family of setters, hand to `hegel_run_start`, then
/// free with `hegel_settings_free`. Settings can be reused across
/// multiple runs; the engine reads them at `hegel_run_start` time.
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
    Done(Result<TestRunResult, String>),
}

/// One in-flight test case handed to the caller by
/// `hegel_next_test_case` (borrowed from the run) or constructed
/// standalone by `hegel_test_case_from_blob` (owned by the caller). The
/// caller drives it with the per-test-case primitives (`hegel_generate`,
/// `hegel_start_span` / `hegel_stop_span`, `hegel_target`, the collection
/// primitives) and concludes it with `hegel_mark_complete`. A run-owned
/// handle becomes invalid once marked complete; calling
/// `hegel_next_test_case` again returns the next test case (or NULL when
/// the run is finished). A standalone handle must be released with
/// `hegel_test_case_free`.
pub struct HegelTestCase {
    ds: Box<dyn DataSource + Send + Sync>,
    is_final: bool,
    completed: bool,
    /// Backing buffer for the borrowed `out_value_cbor` pointer returned
    /// from `hegel_generate`. Re-allocated per call; the previous draw's
    /// bytes are invalidated on the next `hegel_generate`.
    last_value: Vec<u8>,
    /// `Some` for a test case pumped out of a run's worker thread (the
    /// worker blocks on this ack until `hegel_mark_complete`); `None` for
    /// a standalone test case from `hegel_test_case_from_blob`. Doubles as
    /// the ownership marker: `None` means the caller owns the allocation
    /// and must free it with `hegel_test_case_free`.
    ack: Option<mpsc::Sender<()>>,
}

/// In-flight property-test run.
///
/// `hegel_run_start` returns one of these. The caller pulls test cases
/// out via `hegel_next_test_case` until it returns NULL, then reads the
/// aggregated outcome via `hegel_run_result`, and finally frees the
/// handle with `hegel_run_free`. The engine runs on a separate worker
/// thread inside libhegel; the handle owns the channel that ferries
/// test cases between caller and worker.
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

/// Aggregated outcome of a finished run, returned by
/// `hegel_run_result`. Read the passed / failed / errored status via
/// `hegel_run_result_status`, the number of distinct failures via
/// `hegel_run_result_failure_count`, each failure via
/// `hegel_run_result_failure(r, i)`, and — for an errored run — the
/// run-level error message via `hegel_run_result_error`. The pointer is
/// borrowed from the `hegel_run_t` and stays valid until `hegel_run_free`
/// is called.
pub struct HegelRunResult {
    failures: Vec<HegelFailure>,
    /// `Some` iff the run ended in a run-level error instead of a verdict.
    error: Option<CString>,
}

/// One distinct failure surfaced by the run. The strings are owned by
/// the parent `hegel_run_result_t`; reading them via
/// `hegel_failure_panic_message` / `_origin` returns `const char*`
/// pointers that stay valid until `hegel_run_free`.
pub struct HegelFailure {
    panic_message: CString,
    origin: CString,
    /// Base64 failure blob encoding the minimal counterexample's choice
    /// sequence, or `None` when the engine produced no blob. Read via
    /// `hegel_failure_reproduction_blob`.
    reproduce_blob: Option<CString>,
}

impl From<Failure> for HegelFailure {
    fn from(f: Failure) -> Self {
        HegelFailure {
            panic_message: cstring_lossy(&f.panic_message),
            origin: cstring_lossy(&f.origin),
            // The base64 alphabet has no NUL, so this is an
            // invariant: error loudly if it's ever broken.
            reproduce_blob: f
                .reproduce_blob
                .map(|b| CString::new(b).expect("reproduce blob is base64 and contains no NUL")),
        }
    }
}

impl From<TestRunResult> for HegelRunResult {
    fn from(r: TestRunResult) -> Self {
        HegelRunResult {
            failures: r.failures.into_iter().map(HegelFailure::from).collect(),
            error: None,
        }
    }
}

impl HegelRunResult {
    /// A run that ended in a run-level error: no failures, with the
    /// message exposed via `hegel_run_result_error`.
    fn from_error(message: &str) -> Self {
        HegelRunResult {
            failures: Vec::new(),
            error: Some(cstring_lossy(message)),
        }
    }

    fn status(&self) -> hegel_run_status_t {
        if self.error.is_some() {
            hegel_run_status_t::HEGEL_RUN_STATUS_ERROR
        } else if self.failures.is_empty() {
            hegel_run_status_t::HEGEL_RUN_STATUS_PASSED
        } else {
            hegel_run_status_t::HEGEL_RUN_STATUS_FAILED
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

/// Allocate a new settings handle initialised with libhegel's defaults
/// (100 test cases, all phases enabled, normal verbosity, no seed,
/// the default disk database under `.hegel/`). Must be paired with a
/// `hegel_settings_free` call. Never returns NULL.
#[unsafe(no_mangle)]
pub extern "C" fn hegel_settings_new() -> *mut HegelSettings {
    Box::into_raw(Box::new(HegelSettings {
        inner: Settings::new(),
        database_key: None,
    }))
}

/// Free a settings handle previously returned by `hegel_settings_new`.
/// Safe to call with NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_free(s: *mut HegelSettings) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) });
    }
}

unsafe fn settings_mut<'a>(s: *mut HegelSettings) -> Option<&'a mut Settings> {
    unsafe { s.as_mut() }.map(|h| &mut h.inner)
}

/// Set whether the engine should drive a full run loop or stop after
/// one test case. See `hegel_mode_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_mode(s: *mut HegelSettings, mode: hegel_mode_t) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        let m = match mode {
            hegel_mode_t::HEGEL_MODE_TEST_RUN => Mode::TestRun,
            hegel_mode_t::HEGEL_MODE_SINGLE_TEST_CASE => Mode::SingleTestCase,
        };
        *inner = inner.clone().mode(m);
    }
}

/// Maximum number of valid test cases to run before declaring the
/// property held. The default is 100. Note that this counts *valid*
/// cases — assumed-rejected ones don't count against the budget, but
/// see `HEGEL_HC_FILTER_TOO_MUCH` for the limit on consecutive
/// rejections.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_test_cases(s: *mut HegelSettings, n: u64) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().test_cases(n);
    }
}

/// Set the engine's output verbosity. See `hegel_verbosity_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_verbosity(s: *mut HegelSettings, v: hegel_verbosity_t) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        let verbosity = match v {
            hegel_verbosity_t::HEGEL_VERBOSITY_QUIET => Verbosity::Quiet,
            hegel_verbosity_t::HEGEL_VERBOSITY_NORMAL => Verbosity::Normal,
            hegel_verbosity_t::HEGEL_VERBOSITY_VERBOSE => Verbosity::Verbose,
            hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG => Verbosity::Debug,
        };
        *inner = inner.clone().verbosity(verbosity);
    }
}

/// Set the RNG seed. When `has_seed = true`, `seed` is used to
/// initialise generation; when `has_seed = false`, the engine picks a
/// fresh random seed at run start (the default). Combined with
/// `hegel_settings_derandomize(s, true)` this gives reproducible runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_seed(s: *mut HegelSettings, seed: u64, has_seed: bool) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().seed(if has_seed { Some(seed) } else { None });
    }
}

/// Make the run reproducible: derive the seed from a stable hash of
/// `database_key` instead of fresh randomness when no explicit seed is
/// supplied. Useful in CI where you want runs of the same test to be
/// deterministic but different tests to still see different inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_derandomize(s: *mut HegelSettings, derandomize: bool) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().derandomize(derandomize);
    }
}

/// When `yes = true` (the default), the engine keeps generating after
/// the first failure to surface additional *distinct* bugs (different
/// origins), and the final `hegel_run_result_t` lists all of them.
/// When `false`, the run stops after the first failing example.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_report_multiple_failures(s: *mut HegelSettings, yes: bool) {
    if let Some(inner) = unsafe { settings_mut(s) } {
        *inner = inner.clone().report_multiple_failures(yes);
    }
}

/// Configure the on-disk example database used by `HEGEL_PHASE_REUSE`
/// and the auto-persistence path.
///
/// - `database = NULL` → leave at the current value (default
///   `.hegel/examples/` next to the cwd).
/// - `database = ""` → disable the database entirely. Replay phase
///   becomes a no-op and discovered failures are not persisted.
/// - Otherwise → use the directory at `database` as the database root.
///   The directory is created lazily.
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
pub unsafe extern "C" fn hegel_settings_database_key(s: *mut HegelSettings, key: *const c_char) {
    let Some(hs) = (unsafe { s.as_mut() }) else {
        return;
    };
    if key.is_null() {
        hs.database_key = None;
        return;
    }
    match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(k) => hs.database_key = Some(k.to_string()),
        Err(_) => set_last_error("hegel_settings_database_key: key is not valid UTF-8"),
    }
}

/// Enable a specific set of phases via a `HEGEL_PHASE_*` bitmask.
/// Phases not listed in the bitmask are disabled. The default is
/// `HEGEL_PHASE_ALL`. Setting this to 0 produces a run that does
/// nothing.
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

/// Suppress (disable) the health checks listed in the `HEGEL_HC_*`
/// bitmask. The default is "no suppression"; use this when you know a
/// check is going to fire and accept the underlying behavior (e.g. you
/// intentionally have a high rejection rate).
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

static WORKER_PANIC_HOOK: Once = Once::new();

/// The name given to the engine worker thread spawned by `hegel_run_start`.
/// Used both when building the thread and by the panic hook to recognise
/// which panics to swallow.
const WORKER_THREAD_NAME: &str = "hegel-worker";

/// Install (once) a process-global panic hook that swallows the default
/// `thread '…' panicked at <file>:<line>:<col>` stderr line for panics
/// raised on the engine worker thread.
///
/// Every engine panic (an internal invariant, an invalid-argument usage
/// error) is raised on the worker thread, is already caught by the
/// worker's `catch_unwind`, and is surfaced as a run-level error through
/// `hegel_run_result_error`. Letting the default hook *also* dump a
/// Rust-internal source location to the embedding process's stderr is pure
/// noise — a C consumer has no use for `src/native/test_runner.rs:329:21`,
/// and it leaks implementation detail. Panics on any other thread (notably
/// the caller's own thread) fall through to the previous hook unchanged.
fn install_worker_panic_hook() {
    WORKER_PANIC_HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some(WORKER_THREAD_NAME) {
                return;
            }
            prev(info);
        }));
    });
}

/// Start a property-test run with the given settings. Returns a handle
/// the caller pulls test cases out of via `hegel_next_test_case`.
///
/// The engine runs on a worker thread inside libhegel; this function
/// returns immediately after spawning it. The caller does not need to
/// hold the settings handle alive — `hegel_run_start` snapshots the
/// settings it needs.
///
/// Returns NULL on failure with a diagnostic in
/// `hegel_last_error_message`. The returned handle must be freed with
/// `hegel_run_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_start(settings: *const HegelSettings) -> *mut HegelRun {
    clear_last_error();
    install_worker_panic_hook();
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
        .name(WORKER_THREAD_NAME.to_string())
        .spawn(move || {
            // Run-level errors (failed health checks, nondeterminism)
            // come back as `Err` from `run_native`; engine *panics*
            // (internal invariants) would otherwise unwind the worker,
            // drop the sender, and surface as a generic "worker
            // terminated" error, losing the message. Both feed the same
            // run-level error channel (`hegel_run_result_error`).
            let engine = std::panic::AssertUnwindSafe(|| {
                run_native(&settings, database_key.as_deref(), |ds, is_final| {
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
                        // Caller dropped — recover the data source we just
                        // tried to hand off and mark it complete so the
                        // engine can make progress to its (now-irrelevant)
                        // end.
                        if let WorkerMessage::TestCase { ds, .. } = returned {
                            ds.mark_complete(&TestCaseResult::Valid);
                        }
                        return;
                    }
                    // Caller dropping the ack sender is treated the same as
                    // a successful ack — we're winding down regardless.
                    let _ = ack_rx.recv();
                })
            });
            let result = match std::panic::catch_unwind(engine) {
                Ok(Ok(r)) => Ok(r),
                Ok(Err(run_error)) => Err(run_error.to_string()),
                Err(payload) => Err(format!(
                    "Engine panic: {}",
                    hegel::run_lifecycle::panic_message(&payload)
                )),
            };
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

/// Block until the engine produces the next test case, returning a
/// borrowed handle pointing into the parent `hegel_run_t`.
///
/// The caller must complete the previous test case (via
/// `hegel_mark_complete`) before requesting the next one — otherwise
/// this returns NULL and sets `hegel_last_error_message`.
///
/// Returns NULL when the run is finished; call `hegel_run_result` to
/// read the outcome. A NULL with `hegel_last_error_message` set means
/// something went wrong (engine crash, caller misuse) rather than
/// normal completion.
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
                ack: Some(ack),
            });
            let ptr = (&*tc) as *const HegelTestCase as *mut HegelTestCase;
            run.current_tc = Some(tc);
            ptr
        }
        Ok(WorkerMessage::Done(r)) => {
            run.result = Some(match r {
                Ok(r) => HegelRunResult::from(r),
                Err(message) => HegelRunResult::from_error(&message),
            });
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

/// Return the aggregated result of a finished run, borrowed from the
/// parent `hegel_run_t`. Returns NULL with
/// `hegel_last_error_message` set if the run hasn't finished yet
/// (`hegel_next_test_case` has not yet returned NULL on this run).
///
/// The pointer is valid until `hegel_run_free`.
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

/// Free a run handle and its result. Safe to call with NULL.
///
/// If the caller exited its test loop early (e.g. with a still-active
/// test case), this drains the worker thread cleanly: any in-flight
/// test case is marked complete, the abort flag is set so the worker
/// short-circuits, and the worker is joined before the handle is
/// destroyed.
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
            if let Some(ack) = &tc.ack {
                let _ = ack.send(());
            }
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

// ─── Standalone test cases (failure-blob replay) ────────────────────────────

/// Build a standalone test case that replays the example encoded in a
/// base64 failure blob (obtained from `hegel_failure_reproduction_blob` on a
/// prior run).
///
/// There is no run handle and no engine worker: the caller drives the
/// returned test case with the usual per-test-case primitives
/// (`hegel_generate`, spans, …), concludes it with `hegel_mark_complete`,
/// and decides for itself whether the blob reproduced the failure (the
/// property failed again) or is stale (it passed). Replay several blobs by
/// calling this once per blob. A blob whose choices no longer match the
/// caller's generators surfaces as `HEGEL_E_STOP_TEST` from the draw that
/// overruns. `hegel_test_case_is_final_replay` reports true: the replayed
/// example *is* the counterexample.
///
/// Returns NULL with a diagnostic in `hegel_last_error_message` if `s` or
/// `blob` is NULL, or if `blob` is not a valid failure blob (corrupt, or
/// from an incompatible Hegel version). The returned handle is owned by
/// the **caller** — unlike test cases from `hegel_next_test_case`, it must
/// be released with `hegel_test_case_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_from_blob(
    s: *const HegelSettings,
    blob: *const c_char,
) -> *mut HegelTestCase {
    clear_last_error();
    let Some(handle) = (unsafe { s.as_ref() }) else {
        set_last_error("hegel_test_case_from_blob: settings pointer is null");
        return ptr::null_mut();
    };
    if blob.is_null() {
        set_last_error("hegel_test_case_from_blob: blob pointer is null");
        return ptr::null_mut();
    }
    let Ok(blob) = (unsafe { CStr::from_ptr(blob) }).to_str() else {
        set_last_error("hegel_test_case_from_blob: blob is not valid UTF-8");
        return ptr::null_mut();
    };
    let Some(ds) = data_source_for_blob(&handle.inner, blob) else {
        set_last_error(
            "hegel_test_case_from_blob: the supplied failure blob could not be decoded. \
             It may be corrupt or from an incompatible Hegel version.",
        );
        return ptr::null_mut();
    };
    Box::into_raw(Box::new(HegelTestCase {
        ds,
        is_final: true,
        completed: false,
        last_value: Vec::new(),
        ack: None,
    }))
}

/// Free a standalone test case previously returned by
/// `hegel_test_case_from_blob`. Safe to call with NULL (no-op), and safe
/// whether or not the test case was marked complete.
///
/// Must NOT be called on a test case obtained from
/// `hegel_next_test_case` — those are borrowed from the parent
/// `hegel_run_t` and are released by `hegel_run_free`. Passing one here is
/// detected (while the run is still alive) and refused, with a diagnostic
/// in `hegel_last_error_message`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_free(tc: *mut HegelTestCase) {
    clear_last_error();
    if tc.is_null() {
        return;
    }
    if unsafe { (*tc).ack.is_some() } {
        set_last_error(
            "hegel_test_case_free: this test case is owned by its hegel_run_t \
             (it came from hegel_next_test_case); it is freed by hegel_run_free",
        );
        return;
    }
    drop(unsafe { Box::from_raw(tc) });
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
        // A caller-supplied schema was semantically invalid (e.g. an unknown
        // type string). Surface it as HEGEL_E_INVALID_ARG with the diagnostic
        // in hegel_last_error_message — never a panic across the FFI boundary.
        DataSourceError::InvalidArgument(msg) => {
            set_last_error(&msg);
            HEGEL_E_INVALID_ARG
        }
        // `DataSourceError` is `#[non_exhaustive]`; treat any future variant as
        // a backend error rather than failing to compile or panicking.
        other => {
            set_last_error(&other.to_string());
            HEGEL_E_BACKEND
        }
    }
}

/// Draw a value from the test case's data source, using the
/// CBOR-encoded `schema_cbor` to describe its shape (type + bounds +
/// optional category filters, depending on the type).
///
/// On success returns `HEGEL_OK` and writes a borrowed pointer to the
/// CBOR-encoded value into `*out_value_cbor` (length in
/// `*out_value_len`). The pointer is invalidated by the next call into
/// libhegel on this test case — copy the bytes if you need to keep
/// them.
///
/// Returns `HEGEL_E_STOP_TEST` when the engine's choice budget is
/// exhausted for this test case (the caller should abort the body and
/// call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
/// Returns `HEGEL_E_INVALID_ARG` on malformed schema, NULL outputs, or
/// other argument errors; the diagnostic is in
/// `hegel_last_error_message`.
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

/// Open a labeled span around a group of draws so the shrinker can
/// reason about them as a unit. Pair with exactly one
/// `hegel_stop_span(tc, false)` call when the structure is complete.
/// `label` is one of the `HEGEL_LABEL_*` constants.
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

/// Close the most-recently opened span. Pass `discard = true` to mark
/// the span as rejected (e.g. a `filter` predicate didn't hold and the
/// engine should retry from before the span opened).
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

/// Start an engine-managed variable-length collection. The engine
/// chooses how many elements to produce; the caller pulls them one at
/// a time by calling `hegel_collection_more` in a loop. Pass
/// `max_size = UINT64_MAX` for no upper bound.
///
/// On success writes the new collection's id into `*out_collection_id`
/// and returns `HEGEL_OK`. The id is opaque; pass it to subsequent
/// `hegel_collection_more` / `hegel_collection_reject` calls.
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

/// Ask whether the engine wants another element in this collection.
/// On success writes `true` or `false` into `*out_more` and returns
/// `HEGEL_OK`. Call in a loop until `*out_more` is `false`, drawing
/// the next element each time.
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

/// Tell the engine the last element it produced for this collection
/// is not acceptable (e.g. would create a duplicate in a set), so it
/// should try a different one. `why` is an optional human-readable
/// rejection reason (NULL is allowed).
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

/// Create a new engine-managed *variable pool* for stateful testing.
///
/// A pool tracks a set of opaque variable ids that the engine can draw
/// from and shrink over — the primitive behind hegel-rust's
/// `stateful::Variables` and `#[hegel::state_machine]`. The caller keeps
/// its own mapping from variable id to the actual value it generated
/// (mirroring how `Variables<T>` holds a `HashMap<i64, T>`).
///
/// On success writes the new pool's id into `*out_pool_id` and returns
/// `HEGEL_OK`. The id is opaque; pass it to subsequent `hegel_pool_add`
/// / `hegel_pool_generate` calls on the *same* test case.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_pool(tc: *mut HegelTestCase, out_pool_id: *mut i64) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_pool_id.is_null() {
        set_last_error("hegel_new_pool: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.ds.new_pool() {
        Ok(id) => {
            unsafe { *out_pool_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(e),
    }
}

/// Register a new variable in the pool. The engine assigns it a fresh
/// id, which the caller associates with the value it just generated.
///
/// On success writes the new variable's id into `*out_variable_id` and
/// returns `HEGEL_OK`. `pool_id` must be an id returned by
/// `hegel_new_pool` on this test case.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_pool_add(
    tc: *mut HegelTestCase,
    pool_id: i64,
    out_variable_id: *mut i64,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error("hegel_pool_add: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.ds.pool_add(pool_id) {
        Ok(id) => {
            unsafe { *out_variable_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(e),
    }
}

/// Draw a variable id from the pool, letting the engine choose (and
/// shrink) which previously-added variable to reuse. When
/// `consume = true` the drawn variable is removed from the pool (model a
/// destructive action); when `false` it stays available for future
/// draws.
///
/// On success writes the chosen variable id into `*out_variable_id` and
/// returns `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` if the pool currently
/// has no active variables — the caller should guard against that (e.g.
/// only draw when it knows it has added at least one variable) or treat
/// it like any other budget-exhaustion outcome.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_pool_generate(
    tc: *mut HegelTestCase,
    pool_id: i64,
    consume: bool,
    out_variable_id: *mut i64,
) -> c_int {
    clear_last_error();
    let tc = match unsafe { tc_mut(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error("hegel_pool_generate: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.ds.pool_generate(pool_id, consume) {
        Ok(id) => {
            unsafe { *out_variable_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(e),
    }
}

/// Record a numeric observation under `label` for the engine's
/// targeting phase to hill-climb toward. Higher values are "more
/// interesting"; the engine biases later test cases toward inputs that
/// produced higher observations under the same label. Has no effect
/// unless `HEGEL_PHASE_TARGET` is enabled. `label` must be non-NULL
/// and valid UTF-8.
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

/// Mark this test case complete with the given status.
///
/// `origin` is used only when `status == HEGEL_STATUS_INTERESTING`; for
/// other statuses it can be NULL. It identifies *which bug* this failure
/// is — two failures with identical origin strings are treated as the
/// same bug and shrunk together; failures with different origins are
/// treated as distinct bugs and the shrink budget is *partitioned*
/// across them.
///
/// This makes the choice of origin string load-bearing for shrinker
/// quality. In particular, bindings that recover from a host-language
/// panic to call this function MUST NOT pass the recovered panic value
/// (or its stringification) as origin if that value depends on the
/// failing draw — every distinct draw would then look like a fresh bug
/// to the engine and the shrinker would never converge.
///
/// The conventional shape is `"Panic at <file>:<line>"` — i.e. derive
/// origin from the *location* of the failing assertion, not the
/// assertion's message. hegel-rust's own panic-to-failure path does
/// exactly this (see `src/run_lifecycle.rs`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_mark_complete(
    tc: *mut HegelTestCase,
    status: hegel_status_t,
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
        hegel_status_t::HEGEL_STATUS_VALID => TestCaseResult::Valid,
        hegel_status_t::HEGEL_STATUS_INVALID => TestCaseResult::Invalid,
        hegel_status_t::HEGEL_STATUS_OVERRUN => TestCaseResult::Overrun,
        hegel_status_t::HEGEL_STATUS_INTERESTING => {
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
                origin: origin_str,
                reproduce_blob: None,
            })
        }
    };

    tc.ds.mark_complete(&outcome);
    if let Some(ack) = &tc.ack {
        let _ = ack.send(());
    }
    tc.completed = true;
    HEGEL_OK
}

/// True iff this test case is the engine's *final replay* of a
/// minimal failing example. Bindings that want to emit verbose draw
/// traces only for the final counterexample (rather than every probe
/// the shrinker tries) gate their tracing on this flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_is_final_replay(tc: *const HegelTestCase) -> bool {
    match unsafe { tc.as_ref() } {
        Some(t) => t.is_final,
        None => false,
    }
}

// ─── Result inspection ──────────────────────────────────────────────────────

/// The run's aggregate status: passed, failed (the property has
/// counterexamples — see `hegel_run_result_failure`), or errored (the run
/// itself failed and produced no verdict — see `hegel_run_result_error`).
/// A NULL `r` reports `HEGEL_RUN_STATUS_ERROR`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_status(r: *const HegelRunResult) -> hegel_run_status_t {
    match unsafe { r.as_ref() } {
        Some(r) => r.status(),
        None => hegel_run_status_t::HEGEL_RUN_STATUS_ERROR,
    }
}

/// The run-level error message when the run ended in an error rather than
/// a verdict on the property — a failed health check (e.g. FilterTooMuch,
/// TooSlow), a nondeterministic test, or an engine panic — or NULL when it
/// completed normally. An errored run has `hegel_run_result_status(r) ==
/// HEGEL_RUN_STATUS_ERROR` and no failures: the error is a failure of the
/// run itself, not a counterexample to the property. The pointer is valid
/// until `hegel_run_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_error(r: *const HegelRunResult) -> *const c_char {
    match unsafe { r.as_ref() } {
        Some(r) => r.error.as_ref().map(|e| e.as_ptr()).unwrap_or(ptr::null()),
        None => ptr::null(),
    }
}

/// Number of *distinct* failures (by origin) the run surfaced. Each
/// can be inspected via `hegel_run_result_failure(r, i)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_failure_count(r: *const HegelRunResult) -> usize {
    match unsafe { r.as_ref() } {
        Some(r) => r.failures.len(),
        None => 0,
    }
}

/// Borrowed pointer to the `index`-th failure (0-based). Returns NULL
/// if `r` is NULL or `index >= hegel_run_result_failure_count(r)`. The
/// pointer is valid until `hegel_run_free` is called on the parent
/// run.
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

/// The failure's panic message — e.g. the assertion text or
/// engine-emitted message like `"FailedHealthCheck: FilterTooMuch — …"`.
/// Returns NULL if `f` is NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_panic_message(f: *const HegelFailure) -> *const c_char {
    match unsafe { f.as_ref() } {
        Some(f) => f.panic_message.as_ptr(),
        None => ptr::null(),
    }
}

/// The failure's origin string — the stable identifier that the
/// shrinker used to group probes for this bug. Returns NULL if `f` is
/// NULL. See `hegel_mark_complete` for what makes a good origin
/// string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_origin(f: *const HegelFailure) -> *const c_char {
    match unsafe { f.as_ref() } {
        Some(f) => f.origin.as_ptr(),
        None => ptr::null(),
    }
}

/// The failure's reproduce blob — a base64 string encoding the minimal
/// counterexample's choice sequence, suitable for deterministic replay via
/// `hegel_test_case_from_blob`. Returns NULL if `f` is NULL or the
/// engine produced no blob for this failure. The pointer is borrowed from the
/// parent `hegel_run_result_t` and stays valid until `hegel_run_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_reproduction_blob(f: *const HegelFailure) -> *const c_char {
    match unsafe { f.as_ref() } {
        Some(f) => match &f.reproduce_blob {
            Some(blob) => blob.as_ptr(),
            None => ptr::null(),
        },
        None => ptr::null(),
    }
}

// ─── Diagnostics ────────────────────────────────────────────────────────────

/// Most recent error message from libhegel on the calling thread, or
/// the empty string if the most recent call succeeded.
///
/// The returned pointer is a borrow into a thread-local buffer and is
/// invalidated by the next libhegel call on this thread — copy the
/// bytes before making another call.
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

/// Libhegel's version, matching the parent `hegeltest` crate's
/// `CARGO_PKG_VERSION` (e.g. `"0.14.12"`). The returned pointer is
/// static and valid for the program's lifetime.
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
