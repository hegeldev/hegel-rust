#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString, c_char};
use std::ptr;
use std::sync::Arc;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use ciborium::Value;
use parking_lot::Mutex;

/// cbindgen:ignore
mod antithesis_detect;
/// cbindgen:ignore
mod backend;
/// cbindgen:ignore
mod cbor_utils;
/// cbindgen:ignore
mod control;
/// cbindgen:ignore
mod embed;
/// cbindgen:ignore
mod native;
/// cbindgen:ignore
mod panic;
/// cbindgen:ignore
mod settings;
/// cbindgen:ignore
mod unicodedata;

/// cbindgen:ignore
#[cfg(feature = "__bench")]
#[doc(hidden)]
pub mod __bench {
    pub use crate::native::bignum::BigInt;
    pub use crate::native::core::choices::{BytesChoice, FloatChoice, IntegerChoice, StringChoice};
    pub use crate::native::intervalsets::IntervalSet;
    pub use crate::native::rng::EngineRng;

    pub fn biased_integer_sample(ic: &IntegerChoice, rng: &mut EngineRng) -> BigInt {
        crate::native::core::state::biased_integer_sample(ic, rng)
    }

    pub fn biased_string_sample(sc: &StringChoice, rng: &mut EngineRng) -> Vec<u32> {
        crate::native::core::state::biased_string_sample(sc, rng)
    }

    pub fn biased_bytes_sample(bc: &BytesChoice, rng: &mut EngineRng) -> Vec<u8> {
        crate::native::core::state::biased_bytes_sample(bc, rng)
    }

    pub fn biased_float_sample(fc: &FloatChoice, rng: &mut EngineRng) -> f64 {
        crate::native::core::state::biased_float_sample(fc, rng)
    }
}

use crate::backend::{DataSource, DataSourceError, Failure, TestCaseResult, TestRunResult};
use crate::embed::{data_source_for_blob, run_native};
use crate::settings::{Backend, HealthCheck, Mode, Phase, Settings, Verbosity};

/// Result of a libhegel call.
///
/// Every entry point returns one of these except `hegel_context_new` (which
/// returns a context) and `hegel_context_last_error` (which returns the message
/// pointer). `HEGEL_OK` is zero; every error is negative, so `result != HEGEL_OK`
/// (or `result < 0`) tests for failure. Anything else a call produces — a
/// handle, a string, a count — is written through a trailing `out_*` parameter.
/// For the error variants that carry a diagnostic, the message is on the call's
/// context — read it with `hegel_context_last_error()`.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[allow(non_camel_case_types)]
pub enum hegel_result_t {
    /// Success.
    HEGEL_OK = 0,

    /// The engine has exhausted its choice budget for this test case and
    /// wants the caller to abort the body and return. Treat the same as a
    /// validly-completed test case.
    HEGEL_E_STOP_TEST = -1,

    /// An `assume` / `reject` precondition failed. The current test case is
    /// invalid and should be discarded.
    HEGEL_E_ASSUME = -2,

    /// The underlying engine reported an error. See
    /// `hegel_context_last_error()` for the diagnostic.
    HEGEL_E_BACKEND = -3,

    /// A handle pointer (`hegel_settings_t*`, `hegel_run_t*`,
    /// `hegel_test_case_t*`, …) was NULL where it must be non-NULL.
    HEGEL_E_INVALID_HANDLE = -4,

    /// An argument other than a handle was invalid — NULL where a value was
    /// required, malformed CBOR, non-UTF-8 string, etc. See
    /// `hegel_context_last_error()` for specifics.
    HEGEL_E_INVALID_ARG = -5,

    /// `hegel_mark_complete` (or a primitive on the same handle) was called
    /// for a test case that has already been completed.
    HEGEL_E_ALREADY_COMPLETE = -6,

    /// Something was read before it was ready: `hegel_next_test_case` was
    /// called without first completing the previous test case with
    /// `hegel_mark_complete`, or `hegel_run_result` was called before the run
    /// finished (`hegel_next_test_case` has not yet reported completion).
    HEGEL_E_NOT_COMPLETE = -7,

    /// An internal invariant failed inside libhegel (e.g. CBOR
    /// re-serialisation). Should not happen in practice; please file a
    /// bug. See `hegel_context_last_error()` for the diagnostic.
    HEGEL_E_INTERNAL = -8,

    /// A single test-case handle was used from two threads at once. Each
    /// handle may be driven by at most one thread at a time; to generate from
    /// several threads, `hegel_test_case_clone` the handle and give each
    /// thread its own clone. (Clones share the underlying test case but have
    /// independent per-handle locks, so they may be driven concurrently.)
    HEGEL_E_CONCURRENT_USE = -9,

    /// `hegel_test_case_free` was called on a clone (a handle produced by
    /// `hegel_test_case_clone`). Only the root test case may be freed; doing
    /// so releases the root and every clone descended from it. Freeing a clone
    /// frees nothing and returns this code.
    HEGEL_E_NOT_ROOT = -10,
}

use hegel_result_t::*;

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

/// Which source of randomness the engine draws from. Set via
/// `hegel_settings_set_backend`.
///
/// - `HEGEL_BACKEND_AUTO`: choose automatically (the default) —
///   `HEGEL_BACKEND_URANDOM` when running inside Antithesis, otherwise
///   `HEGEL_BACKEND_DEFAULT`.
/// - `HEGEL_BACKEND_DEFAULT`: expand a single seeded PRNG. Runs are
///   reproducible from the seed and shrinking / replay work as usual.
/// - `HEGEL_BACKEND_URANDOM`: read fresh entropy from `/dev/urandom` on
///   every draw (falling back to an OS-seeded PRNG on platforms without
///   it). Intended for running under Antithesis, whose fuzzer controls
///   `/dev/urandom`; you almost certainly don't want it otherwise.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_backend_t {
    HEGEL_BACKEND_AUTO = 0,
    HEGEL_BACKEND_DEFAULT = 1,
    HEGEL_BACKEND_URANDOM = 2,
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
/// `hegel_settings_set_verbosity`.
///
/// - `HEGEL_VERBOSITY_QUIET`: nothing besides the final result.
/// - `HEGEL_VERBOSITY_NORMAL`: a short summary line per run (default).
/// - `HEGEL_VERBOSITY_VERBOSE`: per-test-case progress and drawn values,
///   panic diagnostics as they happen.
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

/// A phase of the property-test loop, used as a bit flag for
/// `hegel_settings_set_phases`.
///
/// `hegel_settings_set_phases` takes a bitwise OR of these values (e.g.
/// `HEGEL_PHASE_GENERATE | HEGEL_PHASE_SHRINK`); the phases not included are
/// disabled. The default is `HEGEL_PHASE_ALL`, which is almost always what you
/// want — turning a phase off is mainly useful for debugging or replay tooling.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_phase_t {
    /// Run hard-coded explicit examples (none today, reserved for future use).
    HEGEL_PHASE_EXPLICIT = 1 << 0,
    /// Replay counterexamples persisted from previous runs (requires a
    /// database path + `hegel_settings_set_database_key`).
    HEGEL_PHASE_REUSE = 1 << 1,
    /// Randomly generate fresh test cases up to the `test_cases` budget.
    HEGEL_PHASE_GENERATE = 1 << 2,
    /// Apply hill-climbing toward observed `hegel_target` scores between
    /// generation rounds.
    HEGEL_PHASE_TARGET = 1 << 3,
    /// Shrink discovered failing examples toward minimal counterexamples.
    HEGEL_PHASE_SHRINK = 1 << 4,
    /// Convenience: all five phases enabled. This is the default.
    HEGEL_PHASE_ALL = 0x1F,
}

/// A health check, used as a bit flag for
/// `hegel_settings_set_suppress_health_check`.
///
/// `hegel_settings_set_suppress_health_check` takes a bitwise OR of these values
/// naming the checks to *disable*. The default is "all enabled"; suppress a
/// check only when you understand why it is firing and accept the behavior.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_health_check_t {
    /// Aborts the run if too many draws are rejected via `assume` / `Invalid`
    /// (default threshold: 200 in a row with no valid case).
    HEGEL_HC_FILTER_TOO_MUCH = 1 << 0,
    /// Aborts the run if individual test cases take so long that the overall
    /// run is impractical.
    HEGEL_HC_TOO_SLOW = 1 << 1,
    /// Aborts the run if generated values are so large that retaining them for
    /// shrinking is impractical.
    HEGEL_HC_TEST_CASES_TOO_LARGE = 1 << 2,
    /// Warns if the first generated test case is already disproportionately
    /// large.
    HEGEL_HC_LARGE_INITIAL_TEST_CASE = 1 << 3,
}

/// Identifies what kind of compound structure a span groups, passed to
/// `hegel_start_span` so the shrinker can choose appropriate shrink moves
/// (e.g. shortening lists vs. simplifying individual list elements). Pick
/// whichever label best describes the surrounding context. Mirrors
/// `hegeltest::test_case::labels`.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_label_t {
    /// Outer span around a list / sequence.
    HEGEL_LABEL_LIST = 1,
    /// One element of a list.
    HEGEL_LABEL_LIST_ELEMENT = 2,
    /// Outer span around a set (unordered, no duplicates).
    HEGEL_LABEL_SET = 3,
    /// One element of a set.
    HEGEL_LABEL_SET_ELEMENT = 4,
    /// Outer span around a map / dictionary.
    HEGEL_LABEL_MAP = 5,
    /// One (key, value) entry of a map.
    HEGEL_LABEL_MAP_ENTRY = 6,
    /// Outer span around a tuple / fixed-arity record.
    HEGEL_LABEL_TUPLE = 7,
    /// Outer span around a `one_of` / disjunction; useful so the shrinker
    /// can swap which branch is taken.
    HEGEL_LABEL_ONE_OF = 8,
    /// Outer span around an `optional` (None vs Some(value)).
    HEGEL_LABEL_OPTIONAL = 9,
    /// Outer span around a fixed-shape record (named fields known
    /// statically).
    HEGEL_LABEL_FIXED_DICT = 10,
    /// Outer span around a `flat_map` / monadic dependent draw.
    HEGEL_LABEL_FLAT_MAP = 11,
    /// Outer span around a `filter` / rejection-sampling wrapper.
    HEGEL_LABEL_FILTER = 12,
    /// Outer span around a `map` / pure transformation.
    HEGEL_LABEL_MAPPED = 13,
    /// Outer span around a `sampled_from` / pick-from-collection draw.
    HEGEL_LABEL_SAMPLED_FROM = 14,
    /// Outer span around the variant discriminator of a sum-type draw.
    HEGEL_LABEL_ENUM_VARIANT = 15,
    /// Span around one swarm-testing feature-flag draw. Emitted internally
    /// by the engine's state-machine rule selection
    /// (`hegel_state_machine_next_rule`); callers normally never open this
    /// span themselves.
    HEGEL_LABEL_FEATURE_FLAG = 16,
}

/// Opaque error-reporting context.
///
/// libhegel records the diagnostic for a failed call on a context the caller
/// supplies, rather than in thread-local state. Thread-local error buffers
/// are ill-defined under runtimes (e.g. Go) that migrate a goroutine between
/// OS threads mid-call, so the message could be written on one thread and
/// read on another; an explicit context sidesteps that entirely.
///
/// Create one with `hegel_context_new`, pass it as the first argument to
/// every fallible `hegel_*` call, read the most recent message with
/// `hegel_context_last_error`, and free it with `hegel_context_free`. A
/// context is cheap; the expected usage is one per test (or per thread).
///
/// A single context must not be used concurrently from multiple threads —
/// each fallible call overwrites the stored message, so sharing one across
/// threads is a data race and unsupported. Passing `NULL` wherever a context
/// is accepted is allowed and simply opts out of error messages: the call
/// still returns its usual error code, there is just nothing to read back.
pub struct HegelContext {
    last_error: CString,
}

/// Allocate a new error-reporting context initialised with an empty message.
/// Never returns NULL. Must be paired with a `hegel_context_free` call.
#[unsafe(no_mangle)]
pub extern "C" fn hegel_context_new() -> *mut HegelContext {
    Box::into_raw(Box::new(HegelContext {
        last_error: CString::default(),
    }))
}

/// Free a context previously returned by `hegel_context_new`. Safe to call
/// with NULL (a no-op that returns `HEGEL_OK`). The `ctx` argument is the
/// context being freed; there is no separate error context to report into.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_context_free(ctx: *mut HegelContext) -> hegel_result_t {
    if !ctx.is_null() {
        drop(unsafe { Box::from_raw(ctx) });
    }
    HEGEL_OK
}

/// Most recent error message recorded on `ctx`, or the empty string if the
/// most recent call taking this context succeeded. Returns NULL only when
/// `ctx` itself is NULL.
///
/// This is the error-reporting reader, not a normal `hegel_*` call: it is the
/// one function (besides `hegel_context_new`) that does not follow the
/// `hegel_result_t` + `out_*` convention. It returns the message pointer
/// directly so a caller can read it straight after the call it is diagnosing,
/// and it does not reset the stored message.
///
/// The returned pointer borrows `ctx`'s internal buffer and is invalidated by
/// the next libhegel call that takes the same `ctx` — copy the bytes before
/// making another such call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_context_last_error(ctx: *const HegelContext) -> *const c_char {
    match unsafe { ctx.as_ref() } {
        Some(c) => c.last_error.as_ptr(),
        None => ptr::null(),
    }
}

/// Record `msg` as `ctx`'s most recent error. A NULL `ctx` discards the
/// message (the caller opted out of error reporting).
fn set_last_error(ctx: *mut HegelContext, msg: &str) {
    if let Some(c) = unsafe { ctx.as_mut() } {
        c.last_error = CString::new(msg)
            .unwrap_or_else(|_| CString::new("error message contained NUL").unwrap());
    }
}

/// Reset `ctx`'s error message to empty at the start of a fallible call. A
/// NULL `ctx` is a no-op.
fn clear_last_error(ctx: *mut HegelContext) {
    if let Some(c) = unsafe { ctx.as_mut() } {
        c.last_error = CString::default();
    }
}

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

enum WorkerMessage {
    TestCase {
        ds: Box<dyn DataSource + Send + Sync>,
        ack: mpsc::Sender<()>,
    },
    Done(Result<TestRunResult, String>),
}

/// A `*mut HegelTestCase` stored in a root's clone registry.
///
/// Raw pointers are neither `Send` nor `Sync`, which would stop
/// `FamilyShared` (and so `Arc<FamilyShared>`) from crossing threads. The
/// pointers are only ever dereferenced under the family lock or during the
/// single-threaded free cascade, so it is sound to mark the wrapper
/// `Send + Sync`. A newtype (rather than casting to `usize`) keeps pointer
/// provenance intact for Miri / strict-provenance.
struct ClonePtr(*mut HegelTestCase);
// SAFETY: see the type comment — registry pointers are only touched under the
// family lock or during the free cascade, never raced.
unsafe impl Send for ClonePtr {}
unsafe impl Sync for ClonePtr {}

/// State shared by every handle in a clone *family* — the root produced by
/// `hegel_next_test_case` / `hegel_test_case_from_blob` and every
/// `hegel_test_case_clone` descended from it.
///
/// The data source, completion status, and run ack are family-wide: marking
/// any handle complete marks the whole family, and the underlying
/// `DataSource` is the single connection all handles draw from. Concurrent
/// draws from two clones are memory-safe (the `NativeDataSource` serialises
/// internally) but currently non-deterministic — making concurrent clone use
/// robust is future work.
struct FamilyShared {
    /// The single underlying data source. `Arc` (not `Box`) so every clone
    /// shares one connection.
    ds: Arc<dyn DataSource + Send + Sync>,
    /// Family-wide completion status. Set once via `compare_exchange` so
    /// `mark_complete` runs `ds.mark_complete` and sends the ack exactly once,
    /// no matter which handle reports it.
    completed: AtomicBool,
    /// `Some` for a family rooted in a run's worker thread (the worker blocks
    /// on this ack until completion); `None` for a standalone family from
    /// `hegel_test_case_from_blob`. Sent on (not taken from) by the handle
    /// that wins the completion `compare_exchange`, so it stays `Some` as a
    /// stable run-owned marker that `hegel_test_case_free` uses to refuse a
    /// run-owned root. The `Mutex` is only here because `mpsc::Sender` is not
    /// `Sync`; sending under it is sound, and send-once is guaranteed by the
    /// completion `compare_exchange`, not the lock.
    ack: Mutex<Option<mpsc::Sender<()>>>,
    /// Every non-root handle cloned from this family, in creation order. The
    /// root owns these allocations: freeing the root frees them all (the free
    /// cascade). Clones cannot be freed individually.
    clones: Mutex<Vec<ClonePtr>>,
}

/// Per-handle state guarded by the handle's own lock.
struct LocalState {
    /// Backing buffer for the borrowed `out_value_cbor` pointer returned from
    /// `hegel_generate`. Re-allocated per call; the previous draw's bytes are
    /// invalidated on the next `hegel_generate` *on this handle*. Per-handle
    /// (not family-wide) so two clones drawing at once don't stomp each
    /// other's returned buffers.
    last_value: Vec<u8>,
}

/// One in-flight test-case handle handed to the caller by
/// `hegel_next_test_case` (borrowed from the run), constructed standalone by
/// `hegel_test_case_from_blob` (owned by the caller), or cloned from another
/// handle by `hegel_test_case_clone`. The caller drives it with the
/// per-test-case primitives (`hegel_generate`, `hegel_start_span` /
/// `hegel_stop_span`, `hegel_target`, the collection primitives) and concludes
/// it with `hegel_mark_complete`.
///
/// A single handle must be driven by at most one thread at a time: each
/// primitive `try_lock`s the handle's own `local`, returning
/// `HEGEL_E_CONCURRENT_USE` on contention. To draw from several threads, clone
/// the handle with `hegel_test_case_clone` and give each thread its own clone;
/// clones share the family but have independent locks.
///
/// A run-owned root becomes invalid once the family is marked complete;
/// calling `hegel_next_test_case` again returns the next test case (or NULL
/// when the run is finished). A standalone root must be released with
/// `hegel_test_case_free`, which also frees every clone in the family.
pub struct HegelTestCase {
    family: Arc<FamilyShared>,
    local: Mutex<LocalState>,
    /// `true` for the family root, `false` for a clone. Only the root may be
    /// freed; freeing it runs the cascade over `family.clones`.
    is_root: bool,
}

/// Box `value` and leak it to a raw pointer for the C ABI.
///
/// The `Send + Sync` bound is the point: every `HegelTestCase` is allocated
/// through here, so it is a compile-time check that the handle stays
/// `Send + Sync` (its `Arc<FamilyShared>` shared, its `Mutex`es `Sync`, its
/// `ClonePtr` registry marked above). The C consumer relies on that when it
/// moves a handle, or shares a family, between threads.
fn into_raw_send_sync<T: Send + Sync>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
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
    // The current test case.
    //
    // This is a logically owned pointer, and would be
    // `Option<Box<HegelTestCase>>` but for the fact that `Box` also asserts
    // noalias (i.e., that there are no mutable references that aren't derived
    // from `current_tc`). Since the caller mutates the current test case
    // through a different pointer, we use a raw pointer instead.
    current_tc: Option<*mut HegelTestCase>,
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

/// One distinct interesting test case surfaced by the run. The strings are
/// owned by the parent `hegel_run_result_t`; reading them via
/// `hegel_failure_origin` / `_reproduction_blob` returns `const char*`
/// pointers that stay valid until `hegel_run_free`.
///
/// A failure carries the origin the engine grouped on and the reproduce blob.
/// The caller replays the blob (via `hegel_test_case_from_blob`) to produce
/// the diagnostic and re-raise the test's own failure.
pub struct HegelFailure {
    origin: CString,
    /// Base64 failure blob encoding the minimal counterexample's choice
    /// sequence, or `None` when the engine produced no blob (a
    /// single-test-case run). Read via `hegel_failure_reproduction_blob`.
    reproduce_blob: Option<CString>,
}

impl From<Failure> for HegelFailure {
    fn from(f: Failure) -> Self {
        HegelFailure {
            origin: cstring_lossy(&f.origin),
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

/// Allocate a new settings handle initialised with libhegel's defaults
/// (100 test cases, all phases enabled, normal verbosity, no seed,
/// the default disk database under `.hegel/`), writing it into
/// `*out_settings`. Must be paired with a `hegel_settings_free` call. Returns
/// `HEGEL_E_INVALID_ARG` if `out_settings` is NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_new(
    ctx: *mut HegelContext,
    out_settings: *mut *mut HegelSettings,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_settings.is_null() {
        set_last_error(ctx, "hegel_settings_new: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let s = Box::into_raw(Box::new(HegelSettings {
        inner: Settings::new(),
        database_key: None,
    }));
    unsafe { *out_settings = s };
    HEGEL_OK
}

/// Free a settings handle previously returned by `hegel_settings_new`.
/// Safe to call with NULL (a no-op that returns `HEGEL_OK`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_free(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
) -> hegel_result_t {
    clear_last_error(ctx);
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) });
    }
    HEGEL_OK
}

/// Resolve a settings handle for a setter, recording a diagnostic and
/// returning `HEGEL_E_INVALID_HANDLE` on a null pointer.
unsafe fn settings_mut<'a>(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    func: &str,
) -> Result<&'a mut HegelSettings, hegel_result_t> {
    match unsafe { s.as_mut() } {
        Some(h) => Ok(h),
        None => {
            set_last_error(ctx, &format!("{func}: settings pointer is null"));
            Err(HEGEL_E_INVALID_HANDLE)
        }
    }
}

/// Set whether the engine should drive a full run loop or stop after
/// one test case. See `hegel_mode_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_mode(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    mode: hegel_mode_t,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_mode") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    let m = match mode {
        hegel_mode_t::HEGEL_MODE_TEST_RUN => Mode::TestRun,
        hegel_mode_t::HEGEL_MODE_SINGLE_TEST_CASE => Mode::SingleTestCase,
    };
    handle.inner = handle.inner.clone().mode(m);
    HEGEL_OK
}

/// Select the engine's randomness backend. See `hegel_backend_t`.
///
/// `HEGEL_BACKEND_AUTO` is the default and leaves the automatic choice in
/// place; `HEGEL_BACKEND_DEFAULT` / `HEGEL_BACKEND_URANDOM` pin an explicit
/// backend, overriding the automatic detection. Like the underlying setting,
/// pinning is one-way: there is no way to un-pin back to AUTO on a handle
/// once an explicit backend has been set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_backend(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    backend: hegel_backend_t,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_backend") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    match backend {
        hegel_backend_t::HEGEL_BACKEND_AUTO => {}
        hegel_backend_t::HEGEL_BACKEND_DEFAULT => {
            handle.inner = handle.inner.clone().backend(Backend::Default);
        }
        hegel_backend_t::HEGEL_BACKEND_URANDOM => {
            handle.inner = handle.inner.clone().backend(Backend::Urandom);
        }
    }
    HEGEL_OK
}

/// Maximum number of valid test cases to run before declaring the
/// property held. The default is 100. Note that this counts *valid*
/// cases — assumed-rejected ones don't count against the budget, but
/// see `HEGEL_HC_FILTER_TOO_MUCH` for the limit on consecutive
/// rejections.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_test_cases(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    n: u64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_test_cases") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    handle.inner = handle.inner.clone().test_cases(n);
    HEGEL_OK
}

/// Set the engine's output verbosity. See `hegel_verbosity_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_verbosity(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    v: hegel_verbosity_t,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_verbosity") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    let verbosity = match v {
        hegel_verbosity_t::HEGEL_VERBOSITY_QUIET => Verbosity::Quiet,
        hegel_verbosity_t::HEGEL_VERBOSITY_NORMAL => Verbosity::Normal,
        hegel_verbosity_t::HEGEL_VERBOSITY_VERBOSE => Verbosity::Verbose,
        hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG => Verbosity::Debug,
    };
    handle.inner = handle.inner.clone().verbosity(verbosity);
    HEGEL_OK
}

/// Set the RNG seed. When `has_seed = true`, `seed` is used to
/// initialise generation; when `has_seed = false`, the engine picks a
/// fresh random seed at run start (the default). Combined with
/// `hegel_settings_set_derandomize(s, true)` this gives reproducible runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_seed(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    seed: u64,
    has_seed: bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_seed") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    handle.inner = handle
        .inner
        .clone()
        .seed(if has_seed { Some(seed) } else { None });
    HEGEL_OK
}

/// Make the run reproducible: derive the seed from a stable hash of
/// `database_key` instead of fresh randomness when no explicit seed is
/// supplied. Useful in CI where you want runs of the same test to be
/// deterministic but different tests to still see different inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_derandomize(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    derandomize: bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_derandomize") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    handle.inner = handle.inner.clone().derandomize(derandomize);
    HEGEL_OK
}

/// When `yes = true` (the default), the engine keeps generating after
/// the first failure to surface additional *distinct* bugs (different
/// origins), and the final `hegel_run_result_t` lists all of them.
/// When `false`, the run stops after the first failing example.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_report_multiple_failures(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    yes: bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle =
        match unsafe { settings_mut(ctx, s, "hegel_settings_set_report_multiple_failures") } {
            Ok(h) => h,
            Err(rc) => return rc,
        };
    handle.inner = handle.inner.clone().report_multiple_failures(yes);
    HEGEL_OK
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
pub unsafe extern "C" fn hegel_settings_set_database(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    database: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_database") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    if database.is_null() {
        return HEGEL_OK;
    }
    let cstr = unsafe { CStr::from_ptr(database) };
    match cstr.to_str() {
        Ok("") => {
            handle.inner = handle.inner.clone().database(None);
            HEGEL_OK
        }
        Ok(path) => {
            handle.inner = handle.inner.clone().database(Some(path.to_string()));
            HEGEL_OK
        }
        Err(_) => {
            set_last_error(ctx, "hegel_settings_set_database: path is not valid UTF-8");
            HEGEL_E_INVALID_ARG
        }
    }
}

/// Set the database key used to scope stored / replayed examples for this run.
/// `key = NULL` clears it (the default).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_database_key(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    key: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let hs = match unsafe { settings_mut(ctx, s, "hegel_settings_set_database_key") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    if key.is_null() {
        hs.database_key = None;
        return HEGEL_OK;
    }
    match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(k) => {
            hs.database_key = Some(k.to_string());
            HEGEL_OK
        }
        Err(_) => {
            set_last_error(
                ctx,
                "hegel_settings_set_database_key: key is not valid UTF-8",
            );
            HEGEL_E_INVALID_ARG
        }
    }
}

/// Enable a specific set of phases, given as a bitwise OR of `hegel_phase_t`
/// values. Phases not included are disabled. The default is `HEGEL_PHASE_ALL`.
/// Passing 0 produces a run that does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_phases(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    phases: u32,
) -> hegel_result_t {
    use hegel_phase_t::*;
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_phases") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    let mut v = Vec::new();
    if phases & (HEGEL_PHASE_EXPLICIT as u32) != 0 {
        v.push(Phase::Explicit);
    }
    if phases & (HEGEL_PHASE_REUSE as u32) != 0 {
        v.push(Phase::Reuse);
    }
    if phases & (HEGEL_PHASE_GENERATE as u32) != 0 {
        v.push(Phase::Generate);
    }
    if phases & (HEGEL_PHASE_TARGET as u32) != 0 {
        v.push(Phase::Target);
    }
    if phases & (HEGEL_PHASE_SHRINK as u32) != 0 {
        v.push(Phase::Shrink);
    }
    handle.inner = handle.inner.clone().phases(v);
    HEGEL_OK
}

/// Suppress (disable) a set of health checks, given as a bitwise OR of
/// `hegel_health_check_t` values. The default is "no suppression"; use this
/// when you know a check is going to fire and accept the underlying behavior
/// (e.g. you intentionally have a high rejection rate).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_suppress_health_check(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    checks: u32,
) -> hegel_result_t {
    use hegel_health_check_t::*;
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_suppress_health_check") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    let mut v = Vec::new();
    if checks & (HEGEL_HC_FILTER_TOO_MUCH as u32) != 0 {
        v.push(HealthCheck::FilterTooMuch);
    }
    if checks & (HEGEL_HC_TOO_SLOW as u32) != 0 {
        v.push(HealthCheck::TooSlow);
    }
    if checks & (HEGEL_HC_TEST_CASES_TOO_LARGE as u32) != 0 {
        v.push(HealthCheck::TestCasesTooLarge);
    }
    if checks & (HEGEL_HC_LARGE_INITIAL_TEST_CASE as u32) != 0 {
        v.push(HealthCheck::LargeInitialTestCase);
    }
    handle.inner = handle.inner.clone().suppress_health_check(v);
    HEGEL_OK
}

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
                // nocov start
                return;
                // nocov end
            }
            prev(info);
        }));
    });
}

/// Start a property-test run with the given settings, writing a handle the
/// caller pulls test cases out of via `hegel_next_test_case` into `*out_run`.
///
/// The engine runs on a worker thread inside libhegel; this function
/// returns immediately after spawning it. The caller does not need to
/// hold the settings handle alive — `hegel_run_start` snapshots the
/// settings it needs.
///
/// Returns `HEGEL_E_INVALID_ARG` for a NULL `out_run`,
/// `HEGEL_E_INVALID_HANDLE` for a NULL `settings`, or `HEGEL_E_BACKEND` if the
/// worker thread cannot be spawned (with a diagnostic in
/// `hegel_context_last_error`). The handle written to `*out_run` must be freed
/// with `hegel_run_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_start(
    ctx: *mut HegelContext,
    settings: *const HegelSettings,
    out_run: *mut *mut HegelRun,
) -> hegel_result_t {
    clear_last_error(ctx);
    install_worker_panic_hook();
    if out_run.is_null() {
        set_last_error(ctx, "hegel_run_start: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let Some(handle) = (unsafe { settings.as_ref() }) else {
        set_last_error(ctx, "hegel_run_start: settings pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    let settings = handle.inner.clone();
    let database_key = handle.database_key.clone();

    let (to_caller, from_worker) = mpsc::channel::<WorkerMessage>();
    let abort = Arc::new(AtomicBool::new(false));
    let abort_worker = Arc::clone(&abort);

    let worker = thread::Builder::new()
        .name(WORKER_THREAD_NAME.to_string())
        .spawn(move || {
            let engine = std::panic::AssertUnwindSafe(|| {
                run_native(&settings, database_key.as_deref(), |ds| {
                    if abort_worker.load(Ordering::Acquire) {
                        ds.mark_complete(&TestCaseResult::Valid);
                        return;
                    }
                    let (ack_tx, ack_rx) = mpsc::channel();
                    let msg = WorkerMessage::TestCase { ds, ack: ack_tx };
                    if let Err(mpsc::SendError(returned)) = to_caller.send(msg) {
                        // nocov start
                        if let WorkerMessage::TestCase { ds, .. } = returned {
                            ds.mark_complete(&TestCaseResult::Valid);
                        } // nocov end
                        return; // nocov
                    }
                    let _ = ack_rx.recv();
                })
            });
            let result = match std::panic::catch_unwind(engine) {
                Ok(Ok(r)) => Ok(r),
                Ok(Err(run_error)) => Err(run_error.to_string()),
                // nocov start
                Err(payload) => Err(format!(
                    "Engine panic: {}",
                    crate::panic::panic_message(&payload)
                )), // nocov end
            };
            let _ = to_caller.send(WorkerMessage::Done(result));
        });

    let worker = match worker {
        Ok(h) => h,
        // nocov start
        Err(e) => {
            set_last_error(ctx, &format!("hegel_run_start: spawn failed: {}", e));
            return HEGEL_E_BACKEND;
        } // nocov end
    };

    let run = Box::into_raw(Box::new(HegelRun {
        worker: Some(worker),
        from_worker,
        abort,
        current_tc: None,
        result: None,
        drained: false,
    }));
    unsafe { *out_run = run };
    HEGEL_OK
}

/// Block until the engine produces the next test case, writing a borrowed
/// handle pointing into the parent `hegel_run_t` into `*out_test_case`.
///
/// When the run is finished this writes NULL into `*out_test_case` and returns
/// `HEGEL_OK`; call `hegel_run_result` to read the outcome. A non-`HEGEL_OK`
/// code means something went wrong (caller misuse, engine crash) rather than
/// normal completion: `HEGEL_E_NOT_COMPLETE` if the previous test case was not
/// marked complete (call `hegel_mark_complete` first), `HEGEL_E_INVALID_HANDLE`
/// for a NULL `run`, or `HEGEL_E_INVALID_ARG` for a NULL `out_test_case`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_next_test_case(
    ctx: *mut HegelContext,
    run: *mut HegelRun,
    out_test_case: *mut *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_test_case.is_null() {
        set_last_error(ctx, "hegel_next_test_case: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_test_case = ptr::null_mut() };
    let Some(run) = (unsafe { run.as_mut() }) else {
        set_last_error(ctx, "hegel_next_test_case: run pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };

    if let Some(tc) = run.current_tc {
        // SAFETY: `run.current_tc` only ever holds a live pointer created by
        // `Box::into_raw`, and the caller is expected to not be concurrently
        // mutating the test case while calling this function.
        let tc_ref = unsafe { &*tc };
        if !tc_ref.family.completed.load(Ordering::Acquire) {
            set_last_error(
                ctx,
                "hegel_next_test_case: previous test case was not marked complete \
                 (call hegel_mark_complete before requesting the next case)",
            );
            return HEGEL_E_NOT_COMPLETE;
        }
        // At this point, the test case has been marked completed, so...
        //
        // SAFETY: `run.current_tc` is a live family root from `Box::into_raw`;
        // freeing it cascades over any clones the body created, and the caller
        // must not dereference it (or its clones) once freed here.
        unsafe { free_family_root(tc) };
        run.current_tc = None;
    }

    if run.drained {
        return HEGEL_OK;
    }

    match run.from_worker.recv() {
        Ok(WorkerMessage::TestCase { ds, ack }) => {
            let case = HegelTestCase::new_root_ptr(ds, Some(ack));
            run.current_tc = Some(case);
            unsafe { *out_test_case = case };
            HEGEL_OK
        }
        Ok(WorkerMessage::Done(r)) => {
            run.result = Some(match r {
                Ok(r) => HegelRunResult::from(r),
                Err(message) => HegelRunResult::from_error(&message),
            });
            run.drained = true;
            HEGEL_OK
        }
        Err(_) => {
            // nocov start
            run.drained = true;
            set_last_error(ctx, "hegel_next_test_case: worker exited without a result");
            HEGEL_E_BACKEND
            // nocov end
        }
    }
}

/// Write the aggregated result of a finished run, borrowed from the parent
/// `hegel_run_t`, into `*out_result`. Returns `HEGEL_E_NOT_COMPLETE` with
/// `hegel_context_last_error` set if the run hasn't finished yet
/// (`hegel_next_test_case` has not yet reported completion on this run),
/// `HEGEL_E_INVALID_HANDLE` for a NULL `run`, or `HEGEL_E_INVALID_ARG` for a
/// NULL `out_result`.
///
/// The pointer written to `*out_result` is valid until `hegel_run_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result(
    ctx: *mut HegelContext,
    run: *mut HegelRun,
    out_result: *mut *const HegelRunResult,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_result.is_null() {
        set_last_error(ctx, "hegel_run_result: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_result = ptr::null() };
    let Some(run) = (unsafe { run.as_ref() }) else {
        set_last_error(ctx, "hegel_run_result: run pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    match &run.result {
        Some(r) => {
            unsafe { *out_result = r as *const HegelRunResult };
            HEGEL_OK
        }
        None => {
            set_last_error(ctx, "hegel_run_result: run has not finished yet");
            HEGEL_E_NOT_COMPLETE
        }
    }
}

/// Free a run handle and its result. Safe to call with NULL (a no-op that
/// returns `HEGEL_OK`).
///
/// If the caller exited its test loop early (e.g. with a still-active
/// test case), this drains the worker thread cleanly: any in-flight
/// test case is marked complete, the abort flag is set so the worker
/// short-circuits, and the worker is joined before the handle is
/// destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_free(
    ctx: *mut HegelContext,
    run: *mut HegelRun,
) -> hegel_result_t {
    clear_last_error(ctx);
    if run.is_null() {
        return HEGEL_OK;
    }
    let mut run = unsafe { Box::from_raw(run) };

    run.abort.store(true, Ordering::Release);

    if let Some(tc) = run.current_tc.take() {
        // SAFETY: `run.current_tc` is a live family root from `Box::into_raw`.
        // If the caller bailed out of its loop with this case still in flight,
        // claim completion for the family once (releasing the worker's ack so
        // it can wind down), then free the root and any clones the body made.
        {
            let family = unsafe { &(*tc).family };
            if family
                .completed
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                family.ds.mark_complete(&TestCaseResult::Valid);
                if let Some(ack) = &*family.ack.lock() {
                    let _ = ack.send(());
                }
            }
        }
        unsafe { free_family_root(tc) };
    }

    while let Ok(msg) = run.from_worker.recv() {
        if let WorkerMessage::TestCase { ds, ack, .. } = msg {
            // nocov start
            ds.mark_complete(&TestCaseResult::Valid);
            let _ = ack.send(());
            // nocov end
        }
    }

    if let Some(handle) = run.worker.take() {
        let _ = handle.join();
    }
    HEGEL_OK
}

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
/// overruns. Replaying a blob is how a caller performs the *final replay* of
/// a counterexample.
///
/// Returns `HEGEL_E_INVALID_HANDLE` for a NULL `s`, or `HEGEL_E_INVALID_ARG`
/// for a NULL `out_test_case`, a NULL `blob`, or a `blob` that is not a valid
/// failure blob (corrupt, non-UTF-8, or from an incompatible Hegel version),
/// with a diagnostic in `hegel_context_last_error`. The handle written to
/// `*out_test_case` is owned by the **caller** — unlike test cases from
/// `hegel_next_test_case`, it must be released with `hegel_test_case_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_from_blob(
    ctx: *mut HegelContext,
    s: *const HegelSettings,
    blob: *const c_char,
    out_test_case: *mut *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_test_case.is_null() {
        set_last_error(ctx, "hegel_test_case_from_blob: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_test_case = ptr::null_mut() };
    let Some(handle) = (unsafe { s.as_ref() }) else {
        set_last_error(ctx, "hegel_test_case_from_blob: settings pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    if blob.is_null() {
        set_last_error(ctx, "hegel_test_case_from_blob: blob pointer is null");
        return HEGEL_E_INVALID_ARG;
    }
    let Ok(blob) = (unsafe { CStr::from_ptr(blob) }).to_str() else {
        set_last_error(ctx, "hegel_test_case_from_blob: blob is not valid UTF-8");
        return HEGEL_E_INVALID_ARG;
    };
    let Some(ds) = data_source_for_blob(&handle.inner, blob) else {
        set_last_error(
            ctx,
            "hegel_test_case_from_blob: the supplied failure blob could not be decoded. \
             It may be corrupt or from an incompatible Hegel version.",
        );
        return HEGEL_E_INVALID_ARG;
    };
    let tc = HegelTestCase::new_root_ptr(ds, None);
    unsafe { *out_test_case = tc };
    HEGEL_OK
}

/// Free a standalone root test case previously returned by
/// `hegel_test_case_from_blob`, along with every clone descended from it. Safe
/// to call with NULL (a no-op that returns `HEGEL_OK`), and safe whether or not
/// the test case was marked complete.
///
/// Must NOT be called on:
/// - a *clone* (a handle from `hegel_test_case_clone`): only the root may be
///   freed, and doing so frees all of its clones. Passing a clone is refused
///   with `HEGEL_E_NOT_ROOT` and nothing is freed.
/// - a test case obtained from `hegel_next_test_case`: those are owned by the
///   parent `hegel_run_t` and released by `hegel_run_free`. Passing one is
///   refused with `HEGEL_E_INVALID_HANDLE`.
///
/// Either refusal records a diagnostic in `hegel_context_last_error`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_free(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    if tc.is_null() {
        return HEGEL_OK;
    }
    // SAFETY: `tc` is a non-null handle from a `hegel_*` constructor.
    let tc_ref = unsafe { &*tc };
    if !tc_ref.is_root {
        set_last_error(
            ctx,
            "hegel_test_case_free: this test case is a clone (from \
             hegel_test_case_clone); only the root test case may be freed, \
             which frees all of its clones along with it",
        );
        return HEGEL_E_NOT_ROOT;
    }
    if tc_ref.family.ack.lock().is_some() {
        set_last_error(
            ctx,
            "hegel_test_case_free: this test case is owned by its hegel_run_t \
             (it came from hegel_next_test_case); it is freed by hegel_run_free",
        );
        return HEGEL_E_INVALID_HANDLE;
    }
    // SAFETY: a standalone root from `from_blob`; freeing cascades to its
    // clones, and the caller must not use it (or its clones) afterward.
    unsafe { free_family_root(tc) };
    HEGEL_OK
}

/// Clone a test-case handle, writing a new handle that shares the same
/// underlying test case into `*out_test_case`.
///
/// The clone is a *view onto the same test case*, not an independent one: it
/// draws from the same data source, and `hegel_mark_complete` on any handle in
/// the family marks them all complete. Clones exist so a test case can be
/// driven from several threads — each handle has its own lock, so two clones
/// may draw concurrently, whereas using a *single* handle from two threads
/// returns `HEGEL_E_CONCURRENT_USE`. (Concurrent draws across clones are
/// currently non-deterministic; making them robust is future work.)
///
/// Cloning is allowed on a clone (the result shares the same root family) and
/// after the family has completed (the clone simply reports
/// `HEGEL_E_ALREADY_COMPLETE` on use). It does not take the source handle's
/// lock, so a handle may be cloned while another thread is mid-draw on it.
///
/// The new handle is owned by the family **root** and must NOT be passed to
/// `hegel_test_case_free` (that returns `HEGEL_E_NOT_ROOT`); it is released
/// when the root is freed (`hegel_test_case_free` for a `from_blob` root, or
/// `hegel_run_free` / the next `hegel_next_test_case` for a run-owned one).
///
/// Returns `HEGEL_E_INVALID_HANDLE` for a NULL `tc`, or `HEGEL_E_INVALID_ARG`
/// for a NULL `out_test_case`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_clone(
    ctx: *mut HegelContext,
    tc: *const HegelTestCase,
    out_test_case: *mut *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_test_case.is_null() {
        set_last_error(ctx, "hegel_test_case_clone: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_test_case = ptr::null_mut() };
    let Some(src) = (unsafe { tc.as_ref() }) else {
        set_last_error(ctx, "hegel_test_case_clone: test case pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    let clone = into_raw_send_sync(HegelTestCase {
        family: Arc::clone(&src.family),
        local: Mutex::new(LocalState {
            last_value: Vec::new(),
        }),
        is_root: false,
    });
    src.family.clones.lock().push(ClonePtr(clone));
    unsafe { *out_test_case = clone };
    HEGEL_OK
}

impl HegelTestCase {
    /// Allocate a family root from a data source and optional run ack, and
    /// return its raw pointer. `ack` is `Some` for a run-owned root (the
    /// worker blocks on it until completion), `None` for a standalone
    /// (`from_blob`) root.
    fn new_root_ptr(
        ds: Box<dyn DataSource + Send + Sync>,
        ack: Option<mpsc::Sender<()>>,
    ) -> *mut HegelTestCase {
        into_raw_send_sync(HegelTestCase {
            family: Arc::new(FamilyShared {
                ds: Arc::from(ds),
                completed: AtomicBool::new(false),
                ack: Mutex::new(ack),
                clones: Mutex::new(Vec::new()),
            }),
            local: Mutex::new(LocalState {
                last_value: Vec::new(),
            }),
            is_root: true,
        })
    }
}

/// Resolve a test-case handle for a per-test-case primitive, returning the
/// handle and its locked per-instance state.
///
/// Takes a *shared* reference (never `&mut`: two threads racing the same
/// handle pointer would make `&mut` instant UB, whereas `&HegelTestCase` is
/// sound because the type is `Sync`). Errors, in order:
/// - `HEGEL_E_INVALID_HANDLE` for a null pointer,
/// - `HEGEL_E_ALREADY_COMPLETE` if the family is already complete (checked
///   before the lock so completion wins over contention),
/// - `HEGEL_E_CONCURRENT_USE` if this handle is already locked by another
///   thread (each handle may be driven by at most one thread at a time).
unsafe fn tc_guard<'a>(
    tc: *const HegelTestCase,
) -> Result<(&'a HegelTestCase, parking_lot::MutexGuard<'a, LocalState>), hegel_result_t> {
    let tc = unsafe { tc.as_ref() }.ok_or(HEGEL_E_INVALID_HANDLE)?;
    if tc.family.completed.load(Ordering::Acquire) {
        return Err(HEGEL_E_ALREADY_COMPLETE);
    }
    let guard = tc.local.try_lock().ok_or(HEGEL_E_CONCURRENT_USE)?;
    Ok((tc, guard))
}

/// Like [`tc_guard`] but without the completion check: resolve the handle and
/// lock it, returning `HEGEL_E_INVALID_HANDLE` for a null pointer or
/// `HEGEL_E_CONCURRENT_USE` on contention. Used by `hegel_mark_complete`, where
/// completion is the `compare_exchange` itself, not a prior load.
unsafe fn tc_lock<'a>(
    tc: *const HegelTestCase,
) -> Result<(&'a HegelTestCase, parking_lot::MutexGuard<'a, LocalState>), hegel_result_t> {
    let tc = unsafe { tc.as_ref() }.ok_or(HEGEL_E_INVALID_HANDLE)?;
    let guard = tc.local.try_lock().ok_or(HEGEL_E_CONCURRENT_USE)?;
    Ok((tc, guard))
}

/// Free a family root and every clone descended from it.
///
/// SAFETY: `root` must be a live family-root pointer produced by
/// `Box::into_raw`, and every pointer in its `family.clones` registry must be
/// a live clone allocation not freed anywhere else. After this call all of
/// them are dangling.
unsafe fn free_family_root(root: *mut HegelTestCase) {
    let clones = {
        let family = unsafe { &(*root).family };
        std::mem::take(&mut *family.clones.lock())
    };
    for clone in clones {
        drop(unsafe { Box::from_raw(clone.0) });
    }
    drop(unsafe { Box::from_raw(root) });
}

fn translate_ds_error(ctx: *mut HegelContext, e: DataSourceError) -> hegel_result_t {
    match e {
        DataSourceError::StopTest => HEGEL_E_STOP_TEST,
        DataSourceError::Assume => HEGEL_E_ASSUME,
        DataSourceError::InvalidArgument(msg) => {
            set_last_error(ctx, &msg);
            HEGEL_E_INVALID_ARG
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
/// `hegel_context_last_error`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    schema_cbor: *const u8,
    schema_len: usize,
    out_value_cbor: *mut *const u8,
    out_value_len: *mut usize,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, mut local) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if schema_cbor.is_null() && schema_len > 0 {
        set_last_error(ctx, "hegel_generate: schema pointer is null");
        return HEGEL_E_INVALID_ARG;
    }
    if out_value_cbor.is_null() || out_value_len.is_null() {
        set_last_error(ctx, "hegel_generate: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }

    let schema_bytes = unsafe { std::slice::from_raw_parts(schema_cbor, schema_len) };
    let schema: Value = match ciborium::de::from_reader(schema_bytes) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(
                ctx,
                &format!("hegel_generate: malformed CBOR schema: {}", e),
            );
            return HEGEL_E_INVALID_ARG;
        }
    };

    let value = match tc.family.ds.generate(&schema) {
        Ok(v) => v,
        Err(e) => return translate_ds_error(ctx, e),
    };

    local.last_value.clear();
    if let Err(e) = ciborium::ser::into_writer(&value, &mut local.last_value) {
        // nocov start
        set_last_error(
            ctx,
            &format!("hegel_generate: failed to re-serialize value: {}", e),
        ); // nocov end
        return HEGEL_E_INTERNAL; // nocov
    }
    unsafe {
        *out_value_cbor = local.last_value.as_ptr();
        *out_value_len = local.last_value.len();
    }
    HEGEL_OK
}

/// Open a labeled span around a group of draws so the shrinker can
/// reason about them as a unit. Pair with exactly one
/// `hegel_stop_span(tc, false)` call when the structure is complete.
///
/// `label` is a `hegel_label_t` value for one of the well-known structure
/// kinds, but the type is `uint64_t` rather than the enum because the label
/// space is open: callers may pass any stable `u64` to tag their own span
/// kinds (the engine treats unrecognised labels as opaque grouping keys).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_start_span(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    label: u64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.family.ds.start_span(label) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Close the most-recently opened span. Pass `discard = true` to mark
/// the span as rejected (e.g. a `filter` predicate didn't hold and the
/// engine should retry from before the span opened).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_stop_span(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    discard: bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.family.ds.stop_span(discard) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
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
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_size: u64,
    max_size: u64,
    out_collection_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_collection_id.is_null() {
        set_last_error(ctx, "hegel_new_collection: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let max = if max_size == u64::MAX {
        None
    } else {
        Some(max_size)
    };
    match tc.family.ds.new_collection(min_size, max) {
        Ok(id) => {
            unsafe { *out_collection_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Ask whether the engine wants another element in this collection.
/// On success writes `true` or `false` into `*out_more` and returns
/// `HEGEL_OK`. Call in a loop until `*out_more` is `false`, drawing
/// the next element each time.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_more(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    collection_id: i64,
    out_more: *mut bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_more.is_null() {
        set_last_error(ctx, "hegel_collection_more: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.family.ds.collection_more(collection_id) {
        Ok(m) => {
            unsafe { *out_more = m };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Tell the engine the last element it produced for this collection
/// is not acceptable (e.g. would create a duplicate in a set), so it
/// should try a different one. `why` is an optional human-readable
/// rejection reason (NULL is allowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_reject(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    collection_id: i64,
    why: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let why_str = if why.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(why) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => {
                set_last_error(ctx, "hegel_collection_reject: why is not valid UTF-8");
                return HEGEL_E_INVALID_ARG;
            }
        }
    };
    match tc.family.ds.collection_reject(collection_id, why_str) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Create a new engine-managed *variable pool* for stateful testing.
///
/// A pool tracks a set of opaque variable ids that the engine can draw
/// from and shrink over — the primitive behind hegel-rust's
/// `stateful::Pool` and `#[hegel::state_machine]`. The caller keeps
/// its own mapping from variable id to the actual value it generated
/// (mirroring how `Pool<T>` holds a `HashMap<i64, T>`).
///
/// On success writes the new pool's id into `*out_pool_id` and returns
/// `HEGEL_OK`. The id is opaque; pass it to subsequent `hegel_pool_add`
/// / `hegel_pool_generate` calls on the *same* test case.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_pool(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    out_pool_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_pool_id.is_null() {
        set_last_error(ctx, "hegel_new_pool: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.family.ds.new_pool() {
        Ok(id) => {
            unsafe { *out_pool_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
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
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    pool_id: i64,
    out_variable_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error(ctx, "hegel_pool_add: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.family.ds.pool_add(pool_id) {
        Ok(id) => {
            unsafe { *out_variable_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
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
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    pool_id: i64,
    consume: bool,
    out_variable_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error(ctx, "hegel_pool_generate: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.family.ds.pool_generate(pool_id, consume) {
        Ok(id) => {
            unsafe { *out_variable_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Convert a C array of `len` NUL-terminated strings into owned Rust
/// strings, setting `hegel_context_last_error` and returning the error
/// code on a null array (with `len > 0`), a null entry, or a non-UTF-8
/// entry.
unsafe fn names_from_c_array(
    ctx: *mut HegelContext,
    func: &str,
    what: &str,
    names: *const *const c_char,
    len: usize,
) -> Result<Vec<String>, hegel_result_t> {
    if names.is_null() && len > 0 {
        set_last_error(ctx, &format!("{func}: {what} pointer is null"));
        return Err(HEGEL_E_INVALID_ARG);
    }
    let ptrs: &[*const c_char] = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(names, len) }
    };
    let mut out = Vec::with_capacity(len);
    for (i, &p) in ptrs.iter().enumerate() {
        if p.is_null() {
            set_last_error(ctx, &format!("{func}: {what}[{i}] is null"));
            return Err(HEGEL_E_INVALID_ARG);
        }
        match unsafe { CStr::from_ptr(p) }.to_str() {
            Ok(s) => out.push(s.to_string()),
            Err(_) => {
                set_last_error(ctx, &format!("{func}: {what}[{i}] is not valid UTF-8"));
                return Err(HEGEL_E_INVALID_ARG);
            }
        }
    }
    Ok(out)
}

/// Register a *state machine* for engine-owned stateful (rule-based)
/// testing: `num_rules` rules and `num_invariants` invariants, each
/// identified by a NUL-terminated UTF-8 name. The engine owns rule
/// selection — including swarm testing, where each test case enables a
/// random subset of rules (at least one) and selection draws only from
/// that subset. The caller drives execution: it asks
/// `hegel_state_machine_next_rule` which rule to run at each step and
/// applies it.
///
/// On success writes the new machine's id into `*out_state_machine_id`
/// and returns `HEGEL_OK`. The id is opaque; pass it to subsequent
/// `hegel_state_machine_next_rule` calls on the *same* test case.
/// Returns `HEGEL_E_INVALID_ARG` if `num_rules` is zero, or on null /
/// non-UTF-8 names.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_state_machine(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    rule_names: *const *const c_char,
    num_rules: usize,
    invariant_names: *const *const c_char,
    num_invariants: usize,
    out_state_machine_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_state_machine_id.is_null() {
        set_last_error(ctx, "hegel_new_state_machine: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let rules = match unsafe {
        names_from_c_array(
            ctx,
            "hegel_new_state_machine",
            "rule_names",
            rule_names,
            num_rules,
        )
    } {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let invariants = match unsafe {
        names_from_c_array(
            ctx,
            "hegel_new_state_machine",
            "invariant_names",
            invariant_names,
            num_invariants,
        )
    } {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let rule_refs: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
    let invariant_refs: Vec<&str> = invariants.iter().map(|s| s.as_str()).collect();
    match tc.family.ds.new_state_machine(&rule_refs, &invariant_refs) {
        Ok(id) => {
            unsafe { *out_state_machine_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Draw the index of the next rule to run, in `[0, num_rules)`, letting
/// the engine choose (and shrink) the rule sequence. Swarm testing is
/// applied per test case: a random subset of rules is enabled on the
/// first call and selection is restricted to that subset for the rest
/// of the test case, with restrictions that shrink away in minimal
/// counterexamples.
///
/// On success writes the chosen rule index into `*out_rule_index` and
/// returns `HEGEL_OK`. `state_machine_id` must be an id returned by
/// `hegel_new_state_machine` on this test case. Returns
/// `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted
/// (the caller should abort the body and call `hegel_mark_complete`
/// with `HEGEL_STATUS_OVERRUN`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_state_machine_next_rule(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    state_machine_id: i64,
    out_rule_index: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_rule_index.is_null() {
        set_last_error(ctx, "hegel_state_machine_next_rule: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.family.ds.state_machine_next_rule(state_machine_id) {
        Ok(index) => {
            unsafe { *out_rule_index = index };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Draw a single boolean that is `true` with probability `p`. `p`
/// must be in `[0.0, 1.0]`; `p = 0.0` always yields `false` and
/// `p = 1.0` always yields `true` without consuming entropy.
///
/// When `has_forced` is `true` the result is forced to `forced`: the
/// engine still records the choice (so replay and shrinking stay
/// aligned) but consumes no entropy, and the shrinker will not flip it.
/// Forcing `true` with `p = 0.0` or `false` with `p = 1.0` is
/// contradictory and rejected.
///
/// On success writes the drawn value into `*out_value` and returns
/// `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice
/// budget is exhausted for this test case (the caller should abort the
/// body and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
/// Returns `HEGEL_E_INVALID_ARG` for a NULL `out_value`, a `p` outside
/// `[0.0, 1.0]` (including NaN), or a contradictory forced value; the
/// diagnostic is in `hegel_context_last_error`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_primitive_boolean(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    p: f64,
    forced: bool,
    has_forced: bool,
    out_value: *mut bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_value.is_null() {
        set_last_error(ctx, "hegel_primitive_boolean: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc
        .family
        .ds
        .primitive_boolean(p, has_forced.then_some(forced))
    {
        Ok(v) => {
            unsafe { *out_value = v };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Record a numeric observation under `label` for the engine's
/// targeting phase to hill-climb toward. Higher values are "more
/// interesting"; the engine biases later test cases toward inputs that
/// produced higher observations under the same label. Has no effect
/// unless `HEGEL_PHASE_TARGET` is enabled. `label` must be non-NULL
/// and valid UTF-8.
///
/// Returns `HEGEL_E_INVALID_ARG` (with a diagnostic in
/// `hegel_context_last_error`) if `value` is not finite, or if `label`
/// has already been observed on this test case — each label may be
/// recorded at most once per case.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_target(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    value: f64,
    label: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if label.is_null() {
        set_last_error(ctx, "hegel_target: label is null");
        return HEGEL_E_INVALID_ARG;
    }
    let label = match unsafe { CStr::from_ptr(label) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error(ctx, "hegel_target: label is not valid UTF-8");
            return HEGEL_E_INVALID_ARG;
        }
    };
    match tc.family.ds.target_observation(value, label) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
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
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    status: hegel_status_t,
    origin: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_lock(tc) } {
        Ok(pair) => pair,
        Err(rc) => return rc,
    };

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
                        set_last_error(ctx, "hegel_mark_complete: origin is not valid UTF-8");
                        return HEGEL_E_INVALID_ARG;
                    }
                }
            };
            TestCaseResult::Interesting(Failure {
                origin: origin_str,
                reproduce_blob: None,
            })
        }
    };

    if tc
        .family
        .completed
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return HEGEL_E_ALREADY_COMPLETE;
    }
    tc.family.ds.mark_complete(&outcome);
    if let Some(ack) = &*tc.family.ack.lock() {
        let _ = ack.send(());
    }
    HEGEL_OK
}

/// Resolve a run-result handle for a getter, recording a diagnostic and
/// returning `HEGEL_E_INVALID_HANDLE` on a null pointer.
unsafe fn result_ref<'a>(
    ctx: *mut HegelContext,
    r: *const HegelRunResult,
    func: &str,
) -> Result<&'a HegelRunResult, hegel_result_t> {
    match unsafe { r.as_ref() } {
        Some(r) => Ok(r),
        None => {
            set_last_error(ctx, &format!("{func}: result pointer is null"));
            Err(HEGEL_E_INVALID_HANDLE)
        }
    }
}

/// Resolve a failure handle for a getter, recording a diagnostic and
/// returning `HEGEL_E_INVALID_HANDLE` on a null pointer.
unsafe fn failure_ref<'a>(
    ctx: *mut HegelContext,
    f: *const HegelFailure,
    func: &str,
) -> Result<&'a HegelFailure, hegel_result_t> {
    match unsafe { f.as_ref() } {
        Some(f) => Ok(f),
        None => {
            set_last_error(ctx, &format!("{func}: failure pointer is null"));
            Err(HEGEL_E_INVALID_HANDLE)
        }
    }
}

/// Write the run's aggregate status into `*out_status`: passed, failed (the
/// property has counterexamples — see `hegel_run_result_failure`), or errored
/// (the run itself failed and produced no verdict — see
/// `hegel_run_result_error`). Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r`
/// or `HEGEL_E_INVALID_ARG` for a NULL `out_status`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_status(
    ctx: *mut HegelContext,
    r: *const HegelRunResult,
    out_status: *mut hegel_run_status_t,
) -> hegel_result_t {
    clear_last_error(ctx);
    let r = match unsafe { result_ref(ctx, r, "hegel_run_result_status") } {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    if out_status.is_null() {
        set_last_error(ctx, "hegel_run_result_status: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_status = r.status() };
    HEGEL_OK
}

/// Write the run-level error message into `*out_error` when the run ended in
/// an error rather than a verdict on the property — a failed health check
/// (e.g. FilterTooMuch, TooSlow), a nondeterministic test, or an engine panic
/// — or NULL when it completed normally. An errored run has
/// `hegel_run_result_status` of `HEGEL_RUN_STATUS_ERROR` and no failures: the
/// error is a failure of the run itself, not a counterexample to the property.
/// The written pointer is valid until `hegel_run_free`. Returns
/// `HEGEL_E_INVALID_HANDLE` for a NULL `r` or `HEGEL_E_INVALID_ARG` for a NULL
/// `out_error`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_error(
    ctx: *mut HegelContext,
    r: *const HegelRunResult,
    out_error: *mut *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let r = match unsafe { result_ref(ctx, r, "hegel_run_result_error") } {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    if out_error.is_null() {
        set_last_error(ctx, "hegel_run_result_error: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_error = r.error.as_ref().map(|e| e.as_ptr()).unwrap_or(ptr::null()) };
    HEGEL_OK
}

/// Write the number of *distinct* failures (by origin) the run surfaced into
/// `*out_count`. Each can be inspected via `hegel_run_result_failure(r, i)`.
/// Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r` or `HEGEL_E_INVALID_ARG`
/// for a NULL `out_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_failure_count(
    ctx: *mut HegelContext,
    r: *const HegelRunResult,
    out_count: *mut usize,
) -> hegel_result_t {
    clear_last_error(ctx);
    let r = match unsafe { result_ref(ctx, r, "hegel_run_result_failure_count") } {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    if out_count.is_null() {
        set_last_error(ctx, "hegel_run_result_failure_count: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_count = r.failures.len() };
    HEGEL_OK
}

/// Write a borrowed pointer to the `index`-th failure (0-based) into
/// `*out_failure`, or NULL if `index >= hegel_run_result_failure_count(r)`.
/// The pointer is valid until `hegel_run_free` is called on the parent run.
/// Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r` or `HEGEL_E_INVALID_ARG`
/// for a NULL `out_failure`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_failure(
    ctx: *mut HegelContext,
    r: *const HegelRunResult,
    index: usize,
    out_failure: *mut *const HegelFailure,
) -> hegel_result_t {
    clear_last_error(ctx);
    let r = match unsafe { result_ref(ctx, r, "hegel_run_result_failure") } {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    if out_failure.is_null() {
        set_last_error(ctx, "hegel_run_result_failure: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe {
        *out_failure = r
            .failures
            .get(index)
            .map(|f| f as *const HegelFailure)
            .unwrap_or(ptr::null());
    }
    HEGEL_OK
}

/// Write the failure's origin string — the stable identifier the shrinker used
/// to group probes for this bug — into `*out_origin`. See `hegel_mark_complete`
/// for what makes a good origin string. Returns `HEGEL_E_INVALID_HANDLE` for a
/// NULL `f` or `HEGEL_E_INVALID_ARG` for a NULL `out_origin`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_origin(
    ctx: *mut HegelContext,
    f: *const HegelFailure,
    out_origin: *mut *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let f = match unsafe { failure_ref(ctx, f, "hegel_failure_origin") } {
        Ok(f) => f,
        Err(rc) => return rc,
    };
    if out_origin.is_null() {
        set_last_error(ctx, "hegel_failure_origin: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_origin = f.origin.as_ptr() };
    HEGEL_OK
}

/// Write the failure's reproduce blob — a base64 string encoding the minimal
/// counterexample's choice sequence, suitable for deterministic replay via
/// `hegel_test_case_from_blob` — into `*out_blob`, or NULL if the engine
/// produced no blob for this failure. The written pointer is borrowed from the
/// parent `hegel_run_result_t` and stays valid until `hegel_run_free`. Returns
/// `HEGEL_E_INVALID_HANDLE` for a NULL `f` or `HEGEL_E_INVALID_ARG` for a NULL
/// `out_blob`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_reproduction_blob(
    ctx: *mut HegelContext,
    f: *const HegelFailure,
    out_blob: *mut *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let f = match unsafe { failure_ref(ctx, f, "hegel_failure_reproduction_blob") } {
        Ok(f) => f,
        Err(rc) => return rc,
    };
    if out_blob.is_null() {
        set_last_error(
            ctx,
            "hegel_failure_reproduction_blob: out parameter is null",
        );
        return HEGEL_E_INVALID_ARG;
    }
    unsafe {
        *out_blob = match &f.reproduce_blob {
            Some(blob) => blob.as_ptr(),
            None => ptr::null(),
        };
    }
    HEGEL_OK
}

/// Write libhegel's version — matching the parent `hegeltest` crate's
/// `CARGO_PKG_VERSION` (e.g. `"0.14.12"`) — into `*out_version`. The written
/// pointer is static and valid for the program's lifetime. Returns
/// `HEGEL_E_INVALID_ARG` for a NULL `out_version`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_version(
    ctx: *mut HegelContext,
    out_version: *mut *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_version.is_null() {
        set_last_error(ctx, "hegel_version: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    static VERSION: &CStr =
        match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
            Ok(c) => c,
            Err(_) => unreachable!(),
        };
    unsafe { *out_version = VERSION.as_ptr() };
    HEGEL_OK
}

#[cfg(test)]
#[path = "../tests/embedded/lib_tests.rs"]
mod tests;
