#![no_std]
#![allow(clippy::missing_safety_doc)]

extern crate alloc;
#[cfg(any(test, feature = "std"))]
extern crate std;

use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::{CStr, c_char, c_void};
use core::future::Future;
use core::pin::Pin;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, Waker};

use crate::sys::sync::{Mutex, MutexGuard};

/// cbindgen:ignore
mod antithesis_detect;
/// cbindgen:ignore
mod backend;
/// cbindgen:ignore
mod control;
/// cbindgen:ignore
mod embed;
/// cbindgen:ignore
mod exchange;
/// cbindgen:ignore
mod native;
/// cbindgen:ignore
mod settings;
/// cbindgen:ignore
mod sys;
/// cbindgen:ignore
mod unicodedata;

/// cbindgen:ignore
#[cfg(feature = "__bench")]
#[doc(hidden)]
pub mod __bench {
    use alloc::vec::Vec;

    pub use crate::native::bignum::BigInt;
    pub use crate::native::core::choices::{BytesChoice, FloatChoice, IntegerChoice, StringChoice};
    pub use crate::native::intervalsets::IntervalSet;
    pub use crate::native::rng::EngineRng;

    pub fn biased_integer_sample(ic: &IntegerChoice, rng: &mut EngineRng) -> BigInt {
        crate::native::core::state::biased_integer_sample(
            ic,
            rng,
            crate::native::core::GenerationParameters::default(),
        )
        .unwrap()
    }

    pub fn biased_string_sample(sc: &StringChoice, rng: &mut EngineRng) -> Vec<u32> {
        crate::native::core::state::biased_string_sample(sc, rng).unwrap()
    }

    pub fn biased_bytes_sample(bc: &BytesChoice, rng: &mut EngineRng) -> Vec<u8> {
        crate::native::core::state::biased_bytes_sample(bc, rng).unwrap()
    }

    pub fn biased_float_sample(fc: &FloatChoice, rng: &mut EngineRng) -> f64 {
        crate::native::core::state::biased_float_sample(fc, rng).unwrap()
    }
}

use crate::backend::{
    DataSource, DataSourceError, Failure, RunError, TestCaseResult, TestRunResult,
};
use crate::control::hegel_internal_unwrap;
use crate::embed::{data_source_for_blob, run_native_async};
use crate::exchange::CaseExchange;
use crate::native::bignum::BigInt;
use crate::settings::{Backend, HealthCheck, Output, Phase, Settings, Verbosity};

/// Result of a libhegel call. See "Calling convention" in the header
/// preamble.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[allow(non_camel_case_types)]
#[must_use]
pub enum hegel_result_t {
    /// Success.
    HEGEL_OK = 0,

    /// libhegel has exhausted its choice budget for this test case and wants
    /// the caller to abort the body and return.
    HEGEL_E_STOP_TEST = -1,

    /// An `assume` / `reject` precondition failed. The current test case is
    /// invalid and should be discarded.
    HEGEL_E_ASSUME = -2,

    /// The underlying backend reported an error. See
    /// `hegel_context_last_error`.
    HEGEL_E_BACKEND = -3,

    /// A handle pointer was NULL where it must be non-NULL.
    HEGEL_E_INVALID_HANDLE = -4,

    /// An argument other than a handle was invalid.
    HEGEL_E_INVALID_ARG = -5,

    /// `hegel_mark_complete` (or a primitive on the same handle) was called
    /// for a test case that has already been completed.
    HEGEL_E_ALREADY_COMPLETE = -6,

    /// Something was read before it was ready: `hegel_next_test_case`
    /// without first completing the previous test case, or
    /// `hegel_run_result` before the run finished.
    HEGEL_E_NOT_COMPLETE = -7,

    /// An internal invariant failed inside libhegel. Should not happen in
    /// practice. Please file a bug at
    /// <https://github.com/hegeldev/hegel-rust/issues>.
    HEGEL_E_INTERNAL = -8,

    /// A single test-case handle was used from two threads at once. Clone
    /// the handle instead.
    HEGEL_E_CONCURRENT_USE = -9,

    /// A recursive draw exceeded its leaf budget (`hegel_recursion_leaf`).
    /// Unwind the current generation attempt — drawing nothing further for
    /// it — back to where `hegel_new_recursion` was called, then call
    /// `hegel_recursion_retry` to discard the attempt and try again.
    HEGEL_E_RETRY = -10,
}

use hegel_result_t::*;

/// Outcome of a single test case. Passed to `hegel_mark_complete`.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_status_t {
    /// The test body ran to completion without issues.
    HEGEL_STATUS_VALID = 0,
    /// An assumption was violated in this test case.
    HEGEL_STATUS_INVALID = 1,
    /// libhegel ran out of choice budget mid test case, typically because a
    /// draw returned `HEGEL_E_STOP_TEST`. Treat the case as inconclusive.
    HEGEL_STATUS_OVERRUN = 2,
    /// The property failed and this test case is a counterexample.
    HEGEL_STATUS_INTERESTING = 3,
}

/// Which source of randomness the engine draws from. Set via
/// `hegel_settings_set_backend`.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_backend_t {
    /// Choose automatically (the default): urandom when running inside
    /// Antithesis, otherwise the default backend.
    HEGEL_BACKEND_AUTO = 0,
    /// Expand a single seeded PRNG. Runs are reproducible from the seed and
    /// shrinking / replay work as usual.
    HEGEL_BACKEND_DEFAULT = 1,
    /// Read fresh entropy from `/dev/urandom` on every draw, falling back to
    /// an OS-seeded PRNG on platforms without it. Intended for running under
    /// Antithesis, whose fuzzer controls `/dev/urandom`; you almost
    /// certainly don't want it otherwise.
    HEGEL_BACKEND_URANDOM = 2,
}

/// Aggregate outcome of a finished run, read via `hegel_run_result_status`.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum hegel_run_status_t {
    /// The property held across every generated test case.
    HEGEL_RUN_STATUS_PASSED = 0,
    /// The property failed. Inspect each distinct counterexample.
    HEGEL_RUN_STATUS_FAILED = 1,
    /// The run itself failed — a failed health check, a nondeterminism
    /// mismatch, a violated engine invariant — and produced no verdict on
    /// the property. There are no failures to inspect; read the message with
    /// `hegel_run_result_error`.
    HEGEL_RUN_STATUS_ERROR = 2,
    /// The property failed on a run that was declared nondeterministic (a
    /// test case created a state machine with `max_concurrency > 1`). The
    /// failures carry no reproduce blob — there was no shrinking and there
    /// is no final replay — so the caller should report the bug from
    /// whatever it captured while running the discovering test case (the
    /// engine stamps every case of such a run nondeterministic up front,
    /// see `hegel_test_case_is_nondeterministic`, precisely so the caller
    /// captures each case's output as it runs).
    HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC = 3,
}

/// Verbosity of engine-emitted output (logs, per-case traces). Set via
/// `hegel_settings_set_verbosity`.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_verbosity_t {
    /// Nothing besides the final result.
    HEGEL_VERBOSITY_QUIET = 0,
    /// A short summary line per run. The default.
    HEGEL_VERBOSITY_NORMAL = 1,
    /// Per-test-case progress and drawn values, plus panic diagnostics as
    /// they happen.
    HEGEL_VERBOSITY_VERBOSE = 2,
    /// As verbose, plus shrinker trace output.
    HEGEL_VERBOSITY_DEBUG = 3,
}

/// A phase of the property-test loop, used as a bit flag.
///
/// A bitwise OR of these is passed to `hegel_settings_set_phases`. The
/// default is `HEGEL_PHASE_ALL`. Turn a phase off for debugging or replay
/// tooling.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_phase_t {
    /// Run hard-coded explicit examples (none today, reserved for future use).
    HEGEL_PHASE_EXPLICIT = 1 << 0,
    /// Replay counterexamples persisted from previous runs. If a database
    /// path and database key aren't passed, this phase is a no-op.
    HEGEL_PHASE_REUSE = 1 << 1,
    /// Randomly generate fresh test cases up to the `test_cases` budget.
    HEGEL_PHASE_GENERATE = 1 << 2,
    /// Apply hill-climbing toward observed `hegel_target` scores between
    /// generation rounds.
    HEGEL_PHASE_TARGET = 1 << 3,
    /// Shrink discovered failing examples.
    HEGEL_PHASE_SHRINK = 1 << 4,
    /// All five phases enabled. The default.
    HEGEL_PHASE_ALL = 0x1F,
}

/// A health check, used as a bit flag.
///
/// A bitwise OR of these is passed to
/// `hegel_settings_set_suppress_health_check`. The default is all enabled.
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum hegel_health_check_t {
    /// Aborts the run if too many draws are rejected by assumptions.
    HEGEL_HC_FILTER_TOO_MUCH = 1 << 0,
    /// Aborts the run if individual test cases take too long.
    HEGEL_HC_TOO_SLOW = 1 << 1,
    /// Aborts the run if generated values are too large.
    HEGEL_HC_TEST_CASES_TOO_LARGE = 1 << 2,
    /// Warns if the first generated test case is already disproportionately
    /// large.
    HEGEL_HC_LARGE_INITIAL_TEST_CASE = 1 << 3,
}

/// Passed to `hegel_start_span`. libhegel opens spans around its own draws.
/// If your Hegel library opens spans, give them labels libhegel has not
/// reserved below, or shrinking may get slower.
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
    /// Span around one regex string draw. Emitted internally by
    /// `hegel_generate_string`; callers normally never open this span
    /// themselves. Likewise for the other engine-side compound draws below.
    HEGEL_LABEL_REGEX = 17,
    /// Span around one email-address draw (`hegel_generate_string`).
    HEGEL_LABEL_EMAIL = 18,
    /// Span around one URL draw (`hegel_generate_string`).
    HEGEL_LABEL_URL = 19,
    /// Span around one domain-name draw (`hegel_generate_string`).
    HEGEL_LABEL_DOMAIN = 20,
    /// Span around one date draw (`hegel_generate_date`).
    HEGEL_LABEL_DATE = 21,
    /// Span around one time draw (`hegel_generate_time`).
    HEGEL_LABEL_TIME = 22,
    /// Span around one datetime draw (`hegel_generate_datetime`).
    HEGEL_LABEL_DATETIME = 23,
    /// Span around one UUID draw (`hegel_generate_uuid`).
    HEGEL_LABEL_UUID = 24,
    /// Span around one IP-address draw (`hegel_generate_ipv4` /
    /// `hegel_generate_ipv6`).
    HEGEL_LABEL_IP_ADDRESS = 25,
    /// Span around one integer draw (`hegel_generate_integer` /
    /// `hegel_generate_integer_big`). Emitted internally, like every
    /// per-draw label: same-label spans are what the engine's mutation
    /// machinery duplicates to propose repeated values.
    HEGEL_LABEL_INTEGER = 26,
    /// Span around one float draw (`hegel_generate_float`).
    HEGEL_LABEL_FLOAT = 27,
    /// Span around one boolean draw (`hegel_generate_boolean`).
    HEGEL_LABEL_BOOLEAN = 28,
    /// Span around one bytes draw (`hegel_generate_bytes`).
    HEGEL_LABEL_BYTES = 29,
    /// Span around one text string draw (`hegel_generate_string` with a
    /// text generator).
    HEGEL_LABEL_STRING = 30,
    /// Outer span around one stateful-testing rule invocation, grouping all
    /// the draws a single rule makes so the shrinker can delete a whole step
    /// at once. Opened by the frontend's state-machine driver.
    HEGEL_LABEL_STATEFUL_RULE = 31,
    /// Span around one fresh-identifier draw (`hegel_pool_add`). Emitted
    /// internally by the engine.
    HEGEL_LABEL_FRESH_ID = 32,
    /// Span around one choose-from-set draw (`hegel_pool_generate`). Emitted
    /// internally by the engine.
    HEGEL_LABEL_SET_CHOICE = 33,
    /// Span around the concurrency-level draw made by
    /// `hegel_new_state_machine`.
    HEGEL_LABEL_CONCURRENCY = 34,
    /// Span around one sub-value of a recursive generator: the leaf-or-branch
    /// decision plus the drawn content. Every sub-value at every depth uses
    /// this same label, which is what lets the shrinker replace a tree with
    /// one of its own subtrees.
    HEGEL_LABEL_RECURSIVE = 35,
}

/// Per-line output callback, passed to `hegel_run_start` /
/// `hegel_test_case_from_blob` (see there for the full contract). `user_data`
/// is the pointer supplied alongside the callback; `line` is one line of
/// engine output, NUL-terminated UTF-8 of `len` bytes (not counting the
/// terminator) without a trailing newline, valid only for the duration of
/// the call.
#[allow(non_camel_case_types)]
pub type hegel_output_callback_t =
    Option<unsafe extern "C" fn(user_data: *mut c_void, line: *const c_char, len: usize)>;

/// Opaque error-reporting context: holds the diagnostic message of a failed
/// call. Passed as the first argument to nearly every function.
pub struct HegelContext {
    last_error: CString,
}

/// A caller-supplied output callback paired with its `user_data` pointer,
/// as passed to `hegel_run_start` / `hegel_test_case_from_blob`.
#[derive(Copy, Clone)]
struct OutputTarget {
    callback: unsafe extern "C" fn(user_data: *mut c_void, line: *const c_char, len: usize),
    user_data: *mut c_void,
}

// SAFETY: the raw `user_data` pointer is what makes this `!Send + !Sync` by
// default, but the documented contract of the output callback is that it must
// be safe to invoke with this `user_data` from whichever thread drives the
// run, so carrying the pair inside the engine future (which moves with the
// run handle) is sound.
unsafe impl Send for OutputTarget {}
unsafe impl Sync for OutputTarget {}

impl OutputTarget {
    /// Deliver one line of output to this target's callback.
    fn emit(self, line: &str) {
        let line = cstring_lossy(line);
        unsafe { (self.callback)(self.user_data, line.as_ptr(), line.as_bytes().len()) };
    }

    /// The engine-facing [`Output`] that delivers each line to this target.
    ///
    /// The closure captures `self` as a whole (via the by-value `emit`
    /// receiver) rather than its fields individually, which would capture a
    /// bare `*mut c_void` and lose [`OutputTarget`]'s `Send + Sync` impls.
    fn as_output(self) -> Output {
        Output::callback(move |line| self.emit(line))
    }
}

/// The engine [`Output`] destination for a run or blob replay: the supplied
/// callback when one is given, stderr otherwise (including for a NULL
/// `callback`, in which case `user_data` is ignored).
fn output_from_callback(callback: hegel_output_callback_t, user_data: *mut c_void) -> Output {
    match callback {
        Some(callback) => OutputTarget {
            callback,
            user_data,
        }
        .as_output(),
        None => Output::stderr(),
    }
}

/// Returns a new error reporting context initialized with an empty message.
/// Never returns NULL. Must be freed with `hegel_context_free`.
#[unsafe(no_mangle)]
pub extern "C" fn hegel_context_new() -> *mut HegelContext {
    Box::into_raw(Box::new(HegelContext {
        last_error: CString::default(),
    }))
}

/// Parameters:
/// `ctx`: The context being freed. No-op when called with NULL.
///
/// Returns `HEGEL_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_context_free(ctx: *mut HegelContext) -> hegel_result_t {
    if !ctx.is_null() {
        drop(unsafe { Box::from_raw(ctx) });
    }
    HEGEL_OK
}

/// Parameters:
/// `ctx`: The context to read.
///
/// Returns the most recent error message recorded on `ctx`, or the empty
/// string if the most recent call taking `ctx` succeeded. NULL only if `ctx`
/// is NULL. The pointer borrows the context's internal buffer and is
/// invalidated by the next call taking the same context.
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
/// NULL `ctx` is a no-op. Skips the allocation when the message is already
/// empty — this runs at the top of every draw in the hot loop.
fn clear_last_error(ctx: *mut HegelContext) {
    if let Some(c) = unsafe { ctx.as_mut() } {
        if !c.last_error.as_bytes().is_empty() {
            c.last_error = CString::default();
        }
    }
}

/// A settings handle is built up with setters, handed to `hegel_run_start`,
/// and then freed. Settings can be reused across runs.
///
/// A configured handle may be shared across threads, but do not call setters
/// concurrently on the same handle.
pub struct HegelSettings {
    inner: Settings,
    /// Optional database key used by the runner for example storage / replay.
    /// Not part of `Settings` itself in upstream hegel; passed as a separate
    /// argument to `run_native_async` on `hegel_run_start`.
    database_key: Option<String>,
}

/// State shared by every handle in a clone *family* — the handle produced by
/// `hegel_next_test_case` / `hegel_test_case_from_blob` and every
/// `hegel_test_case_clone` descended from it.
///
/// The completion status and run ack are family-wide: marking any handle
/// complete marks the whole family. Each handle draws from its own *stream*
/// data source (see [`HegelTestCase::stream`]) — the root handle from the
/// family's root stream, each clone from the independent stream
/// `hegel_test_case_clone` created for it — so concurrent draws on
/// different handles generate independently and deterministically.
///
/// Every handle owns one `Arc<FamilyShared>` reference; the run keeps its own
/// reference too. The `Arc` strong count is the family's reference count, so
/// the engine state is dropped only once every handle has been freed and the
/// run has released its reference.
struct FamilyShared {
    /// The family's root-stream data source. Every handle keeps the family
    /// alive; the root handle also draws from this source, and completion
    /// (which is family-wide in the engine) is reported through it.
    ds: Arc<dyn DataSource + Send + Sync>,
    /// Family-wide completion status. Set once via `compare_exchange` in
    /// [`Self::complete`] so `ds.mark_complete` runs exactly once, no matter
    /// which handle reports it. For a run-owned family this is also the gate
    /// `hegel_next_test_case` checks before resuming the engine.
    completed: AtomicBool,
}

impl FamilyShared {
    /// Claim family-wide completion. First caller wins: it records `outcome`
    /// on the data source; every later call — a racing clone, or the run
    /// tearing down an in-flight case — is a no-op. This is the single home
    /// of the exactly-once completion protocol.
    fn complete(&self, outcome: &TestCaseResult) {
        if self
            .completed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.ds.mark_complete(outcome);
        }
    }
}

/// Per-handle state guarded by the handle's own lock.
struct LocalState {
    /// Whether `hegel_mark_complete` has already been called on *this* handle.
    /// Completing the family is first-caller-wins and family-wide (see
    /// `FamilyShared::completed`), so a second handle completing is a safe
    /// no-op; but completing the *same* handle twice is a usage error, which
    /// this per-handle flag detects.
    completed: bool,
}

/// A test-case handle is what a test body draws from. The caller drives it
/// with the per-test-case primitives, concludes it with
/// `hegel_mark_complete`, and releases it with `hegel_test_case_free`.
///
/// A test case is a single execution of the test function and the concrete
/// values generated for it. Cloning a handle yields more handles onto the
/// same test case, each with its own choice sequence.
pub struct HegelTestCase {
    family: Arc<FamilyShared>,
    /// The independent stream this handle draws from: the family's root
    /// stream for the root handle, a cloned stream for a
    /// `hegel_test_case_clone` handle.
    stream: Arc<dyn DataSource + Send + Sync>,
    local: Mutex<LocalState>,
}

/// Box `value` and leak it to a raw pointer for the C ABI.
///
/// The `Send + Sync` bound is the point: every `HegelTestCase` is allocated
/// through here, so it is a compile-time check that the handle stays
/// `Send + Sync` (its `Arc<FamilyShared>` shared, its `Mutex`es `Sync`). The C
/// consumer relies on that when it moves a handle, or shares a family, between
/// threads.
fn into_raw_send_sync<T: Send + Sync>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
}

/// The engine future a run drives: the whole exploration (database replay,
/// generation, targeting, shrinking), suspended at each offered test case.
type EngineFuture = Pin<Box<dyn Future<Output = Result<TestRunResult, RunError>> + Send>>;

/// In-flight property-test run.
///
/// The caller starts a run, repeatedly asks for the next test case, reports
/// its outcome, and reads the run result after all test cases have been
/// run.
///
/// The run handle owns the suspended run loop as a future, and each
/// `hegel_next_test_case` call resumes it on the calling thread until it
/// returns the next test case or finishes.
pub struct HegelRun {
    /// The suspended engine. `None` once the run has produced its result.
    engine: Option<EngineFuture>,
    /// The exchange the engine offers each test case's data source through;
    /// the engine future holds the other reference.
    exchange: Arc<CaseExchange>,
    // The run's own reference to the current test case's family.
    //
    // The handle returned to the caller from `hegel_next_test_case` is freed
    // by the caller (via `hegel_test_case_free`); this is a *separate*
    // reference the run holds so the data source stays alive while the run is
    // reading it, and so the caller freeing its handle early does not drop the
    // family. It is released (decrementing the family refcount) when the run
    // advances to the next case or is freed.
    current_family: Option<Arc<FamilyShared>>,
    result: Option<HegelRunResult>,
}

/// A run result is the outcome of a finished run, returned as a
/// caller-owned copy. It stays valid after `hegel_run_free`, and is
/// released separately.
///
/// A failed run produced counterexamples to the property. An errored run
/// produced no verdict on the property at all, so it has no failures to
/// inspect. A run errors on a failed health check, a nondeterminism
/// mismatch, or a violated internal invariant of libhegel.
#[derive(Clone)]
pub struct HegelRunResult {
    failures: Vec<HegelFailure>,
    /// `Some` iff the run ended in a run-level error instead of a verdict.
    error: Option<CString>,
    /// Whether the run was nondeterministic: a failing run then reports
    /// `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC` and its failures carry no
    /// reproduce blob.
    nondeterministic: bool,
}

/// One distinct interesting test case surfaced by the run.
/// `hegel_run_result_failure` writes a caller-owned run result.
/// Reading strings within the run result via `hegel_failure_origin` /
/// `_reproduction_blob` returns `const char*` pointers that stay valid until
/// the memory is released with `hegel_failure_free`. The snapshot is
/// independent of the result and run it came from.
///
/// A failure carries the origin `libhegel` grouped on and the reproduce blob.
/// The caller replays the blob (via `hegel_test_case_from_blob`) to produce
/// the diagnostic and re-raise the test's own failure.
#[derive(Clone)]
pub struct HegelFailure {
    origin: CString,
    /// Base64 failure blob encoding the minimal counterexample's choice
    /// sequence, or `None` when the engine produced no blob (a
    /// nondeterministic run). Read via
    /// `hegel_failure_reproduction_blob`.
    reproduce_blob: Option<CString>,
}

impl From<Failure> for HegelFailure {
    fn from(f: Failure) -> Self {
        HegelFailure {
            origin: cstring_lossy(&f.origin),
            reproduce_blob: f.reproduce_blob.map(|b| cstring_lossy(&b)),
        }
    }
}

impl From<TestRunResult> for HegelRunResult {
    fn from(r: TestRunResult) -> Self {
        HegelRunResult {
            failures: r.failures.into_iter().map(HegelFailure::from).collect(),
            error: None,
            nondeterministic: r.nondeterministic,
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
            nondeterministic: false,
        }
    }

    fn status(&self) -> hegel_run_status_t {
        if self.error.is_some() {
            hegel_run_status_t::HEGEL_RUN_STATUS_ERROR
        } else if self.failures.is_empty() {
            hegel_run_status_t::HEGEL_RUN_STATUS_PASSED
        } else if self.nondeterministic {
            hegel_run_status_t::HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC
        } else {
            hegel_run_status_t::HEGEL_RUN_STATUS_FAILED
        }
    }
}

/// Replace interior NULs (which can't appear in C strings) with the
/// REPLACEMENT CHARACTER. Hegel-produced diagnostic strings shouldn't
/// contain NULs, but defending against that here means the caller never
/// sees a `CString` construction fail.
fn cstring_lossy(s: &str) -> CString {
    let sanitized: String = s
        .chars()
        .map(|c| if c == '\0' { '\u{FFFD}' } else { c })
        .collect();
    let mut bytes = sanitized.into_bytes();
    bytes.push(0);
    // SAFETY: `sanitized` contains no U+0000, and no other char's UTF-8
    // encoding contains a zero byte, so the only NUL in `bytes` is the
    // terminator pushed above.
    unsafe { CString::from_vec_with_nul_unchecked(bytes) }
}

/// Parameters:
/// `out_settings`: Receives a handle initialized with libhegel's
///   defaults: 100 test cases, all phases enabled, normal verbosity, no
///   seed, and the default disk database under `.hegel/`.
///
/// Returns `HEGEL_OK`.
///
/// When a CI environment is detected (via `CI`, `GITHUB_ACTIONS`, and
/// similar variables) the defaults change: the database is disabled and
/// derandomization is enabled. Override either with the explicit setters.
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

/// Parameters:
/// `s`: The handle to free. Safe to call with NULL.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `backend`: A `hegel_backend_t` value selecting the source of
///   randomness.
///
/// Returns `HEGEL_OK`.
///
/// The enum-valued setters take `uint32_t` rather than the enum type so
/// that an out-of-range value is an error instead of undefined behavior.
///
/// Once an explicit backend has been set on a handle there is no way to
/// change it within a run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_backend(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    backend: u32,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_backend") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    match backend {
        x if x == hegel_backend_t::HEGEL_BACKEND_AUTO as u32 => {}
        x if x == hegel_backend_t::HEGEL_BACKEND_DEFAULT as u32 => {
            handle.inner = handle.inner.clone().backend(Backend::Default);
        }
        x if x == hegel_backend_t::HEGEL_BACKEND_URANDOM as u32 => {
            handle.inner = handle.inner.clone().backend(Backend::Urandom);
        }
        _ => {
            set_last_error(
                ctx,
                &format!("hegel_settings_set_backend: unknown backend {backend}"),
            );
            return HEGEL_E_INVALID_ARG;
        }
    }
    HEGEL_OK
}

/// Parameters:
/// `n`: Maximum number of valid test cases to run before declaring the
///   property held. 100 by default. Cases rejected by an assumption do not
///   count against this budget.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `n`: Target number of steps to run per stateful test case. Each stateful
///   case runs at least one step and at most `n`. The default is 50. `n`
///   must be at least 1.
///
/// Returns `HEGEL_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_stateful_step_count(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    n: i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_stateful_step_count") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    if n < 1 {
        set_last_error(
            ctx,
            &format!(
                "hegel_settings_set_stateful_step_count: step count must be at least 1, got {n}"
            ),
        );
        return HEGEL_E_INVALID_ARG;
    }
    handle.inner = handle.inner.clone().stateful_step_count(n);
    HEGEL_OK
}

/// Parameters:
/// `v`: Controls the output verbosity. See `hegel_verbosity_t`.
///
/// Returns `HEGEL_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_verbosity(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    v: u32,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_verbosity") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    let verbosity = match v {
        x if x == hegel_verbosity_t::HEGEL_VERBOSITY_QUIET as u32 => Verbosity::Quiet,
        x if x == hegel_verbosity_t::HEGEL_VERBOSITY_NORMAL as u32 => Verbosity::Normal,
        x if x == hegel_verbosity_t::HEGEL_VERBOSITY_VERBOSE as u32 => Verbosity::Verbose,
        x if x == hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG as u32 => Verbosity::Debug,
        _ => {
            set_last_error(
                ctx,
                &format!("hegel_settings_set_verbosity: unknown verbosity {v}"),
            );
            return HEGEL_E_INVALID_ARG;
        }
    };
    handle.inner = handle.inner.clone().verbosity(verbosity);
    HEGEL_OK
}

/// Parameters:
/// `seed`: The RNG seed to initialize generation with.
/// `has_seed`: When `false` (the default), libhegel picks a fresh random
///   seed at run start.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `derandomize`: Derive the seed from a stable hash of the database key
///   instead of fresh randomness when no explicit seed is set.
///
/// Returns `HEGEL_OK`.
///
/// Useful in CI, where you want repeated runs of one test to be
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

/// Parameters:
/// `yes`: When `true`, libhegel keeps generating after the first failure
///   to surface additional distinct bugs. Failures from different locations
///   in the program are considered distinct bugs. The final result lists
///   all of them. When `false`, the run stops after the first failing
///   example.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `database`: NULL sets it to the default: `./.hegel/examples/`. `""`
///   disables the database entirely. Discovered failures will not be
///   stored. Anything else is used as the database root directory. The
///   directory will be created if it does not already exist.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `key`: Scopes stored and replayed examples. NULL clears it (the
///   default).
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `phases`: A bitwise OR of `hegel_phase_t` values to toggle phases. The
///   default is `HEGEL_PHASE_ALL`.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `checks`: A bitwise OR of `hegel_health_check_t` values naming the
///   checks to toggle. Each call overwrites the previous suppressions.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `settings`: The settings for this run. The caller can free the
///   settings after passing them in since libhegel copies the memory.
/// `callback`: Where libhegel's output for this run goes. NULL leaves
///   output on stderr.
/// `user_data`: Passed through to `callback` verbatim. Ignored when
///   `callback` is NULL.
/// `out_run`: Receives the run handle.
///
/// Returns `HEGEL_OK`.
///
/// This only sets up the run. No test case is generated until the first
/// `hegel_next_test_case` call. libhegel emits while it runs inside that
/// call, so the callback is invoked on whichever thread makes it. Because
/// it runs inside `hegel_next_test_case`, the callback must not call back
/// into libhegel on the same run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_start(
    ctx: *mut HegelContext,
    settings: *const HegelSettings,
    callback: hegel_output_callback_t,
    user_data: *mut c_void,
    out_run: *mut *mut HegelRun,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_run.is_null() {
        set_last_error(ctx, "hegel_run_start: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let Some(handle) = (unsafe { settings.as_ref() }) else {
        set_last_error(ctx, "hegel_run_start: settings pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    let settings = handle
        .inner
        .clone()
        .output(output_from_callback(callback, user_data));
    let database_key = handle.database_key.clone();

    let exchange = Arc::new(CaseExchange::new());
    let engine_exchange = Arc::clone(&exchange);
    let engine: EngineFuture = Box::pin(async move {
        run_native_async(&settings, database_key.as_deref(), &engine_exchange).await
    });

    let run = Box::into_raw(Box::new(HegelRun {
        engine: Some(engine),
        exchange,
        current_family: None,
        result: None,
    }));
    unsafe { *out_run = run };
    HEGEL_OK
}

/// Parameters:
/// `out_test_case`: Receives a handle for the next test case, or NULL
///   once the run is finished.
///
/// Returns `HEGEL_OK`, including at normal completion, where
/// `*out_test_case` is NULL and you should call `hegel_run_result`.
/// `HEGEL_E_NOT_COMPLETE` if the previous test case was not marked
/// complete.
///
/// The handle is owned by the caller and must be released with
/// `hegel_test_case_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_next_test_case(
    ctx: *mut HegelContext,
    run: *mut HegelRun,
    out_test_case: *mut *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    let Some(run) = (unsafe { run.as_mut() }) else {
        set_last_error(ctx, "hegel_next_test_case: run pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    if out_test_case.is_null() {
        set_last_error(ctx, "hegel_next_test_case: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_test_case = ptr::null_mut() };

    if let Some(family) = run.current_family.take() {
        if !family.completed.load(Ordering::Acquire) {
            set_last_error(
                ctx,
                "hegel_next_test_case: previous test case was not marked complete \
                 (call hegel_mark_complete before requesting the next case)",
            );
            run.current_family = Some(family);
            return HEGEL_E_NOT_COMPLETE;
        }
        // The previous case is complete; dropping the run's reference here
        // releases the data source unless the caller still holds a handle to
        // it (in which case it lives until the caller frees that handle).
        drop(family);
    }

    let Some(engine) = run.engine.as_mut() else {
        return HEGEL_OK;
    };

    match poll_engine(engine) {
        Poll::Pending => match run.exchange.take() {
            Ok(ds) => {
                let family = new_family(ds);
                let case = handle_from_family(Arc::clone(&family));
                run.current_family = Some(family);
                unsafe { *out_test_case = case };
                HEGEL_OK
            }
            Err(e) => {
                run.result = Some(HegelRunResult::from_error(&e.to_string()));
                run.engine = None;
                HEGEL_OK
            }
        },
        Poll::Ready(r) => {
            run.result = Some(match r {
                Ok(r) => HegelRunResult::from(r),
                Err(run_error) => HegelRunResult::from_error(&run_error.to_string()),
            });
            run.engine = None;
            HEGEL_OK
        }
    }
}

/// Resume the engine until it offers the next test case (`Pending`) or the
/// run finishes (`Ready`). The engine only suspends at its case exchange and
/// is only resumed here, so a no-op waker suffices — no executor is involved.
fn poll_engine(engine: &mut EngineFuture) -> Poll<Result<TestRunResult, RunError>> {
    engine
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

/// Parameters:
/// `out_result`: Receives a caller-owned copy of the finished run's
///   result.
///
/// Returns `HEGEL_OK`, or `HEGEL_E_NOT_COMPLETE` if the run hasn't finished
/// yet.
///
/// Each call produces a copy, freed separately. It stays valid after
/// `hegel_run_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result(
    ctx: *mut HegelContext,
    run: *mut HegelRun,
    out_result: *mut *mut HegelRunResult,
) -> hegel_result_t {
    clear_last_error(ctx);
    let Some(run) = (unsafe { run.as_ref() }) else {
        set_last_error(ctx, "hegel_run_result: run pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    if out_result.is_null() {
        set_last_error(ctx, "hegel_run_result: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_result = ptr::null_mut() };
    match &run.result {
        Some(r) => {
            unsafe { *out_result = into_raw_send_sync(r.clone()) };
            HEGEL_OK
        }
        None => {
            set_last_error(ctx, "hegel_run_result: run has not finished yet");
            HEGEL_E_NOT_COMPLETE
        }
    }
}

/// Parameters:
/// `r`: The run result to free and the strings read off it. Safe to call
///   with NULL.
///
/// Returns `HEGEL_OK`.
///
/// Must be called exactly once per run result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_free(
    ctx: *mut HegelContext,
    r: *mut HegelRunResult,
) -> hegel_result_t {
    clear_last_error(ctx);
    if r.is_null() {
        return HEGEL_OK;
    }
    // SAFETY: `r` is a non-null snapshot from `hegel_run_result` that the
    // caller is freeing exactly once.
    drop(unsafe { Box::from_raw(r) });
    HEGEL_OK
}

/// Parameters:
/// `run`: The run to free. Safe to call with NULL.
///
/// Returns `HEGEL_OK`.
///
/// If the caller exited its loop early, any in-flight test case is marked
/// complete and the rest of the exploration is dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_free(
    ctx: *mut HegelContext,
    run: *mut HegelRun,
) -> hegel_result_t {
    clear_last_error(ctx);
    if run.is_null() {
        return HEGEL_OK;
    }
    let run = unsafe { Box::from_raw(run) };

    if let Some(family) = run.current_family.as_ref() {
        // If the caller bailed out of its loop with this case still in flight,
        // claim completion for the family so any handles the caller still
        // holds observe a concluded case. Dropping the run's reference (as
        // part of dropping the run below) releases the data source unless the
        // caller still holds a handle to it, in which case it lives until the
        // caller frees that handle.
        family.complete(&TestCaseResult::Valid);
    }

    // Dropping the run drops the suspended engine future, cancelling the rest
    // of the exploration at its suspension point.
    drop(run);
    HEGEL_OK
}

/// A library uses a reproduce blob to replay a counterexample: it reruns
/// the minimal failing test case so it can display the drawn values and
/// re-raise the test's own failure.
///
/// There is no run handle and no run loop involved. The caller drives the
/// returned test case with the usual per-test-case primitives, concludes it
/// with `hegel_mark_complete`, and decides for itself whether the blob
/// reproduced the failure (the property failed again) or is stale/flaky (it
/// passed).
///
/// Parameters:
/// `blob`: A base64 blob from `hegel_failure_reproduction_blob`.
/// `callback` / `user_data`: Where this replay's output goes, with the
///   same contract as `hegel_run_start`. The callback is only ever invoked
///   on this thread and need not outlive the call.
/// `out_test_case`: Receives a caller-owned test-case handle. Released
///   like any other with `hegel_test_case_free`.
///
/// Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for a blob that is not
/// valid (corrupt, non-UTF-8, or from an incompatible Hegel version).
///
/// A blob whose choices no longer match the caller's generators returns
/// `HEGEL_E_STOP_TEST` from the draw that overruns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_from_blob(
    ctx: *mut HegelContext,
    s: *const HegelSettings,
    blob: *const c_char,
    callback: hegel_output_callback_t,
    user_data: *mut c_void,
    out_test_case: *mut *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    let Some(handle) = (unsafe { s.as_ref() }) else {
        set_last_error(ctx, "hegel_test_case_from_blob: settings pointer is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    if out_test_case.is_null() {
        set_last_error(ctx, "hegel_test_case_from_blob: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_test_case = ptr::null_mut() };
    if blob.is_null() {
        set_last_error(ctx, "hegel_test_case_from_blob: blob pointer is null");
        return HEGEL_E_INVALID_ARG;
    }
    let Ok(blob) = (unsafe { CStr::from_ptr(blob) }).to_str() else {
        set_last_error(ctx, "hegel_test_case_from_blob: blob is not valid UTF-8");
        return HEGEL_E_INVALID_ARG;
    };
    let settings = handle
        .inner
        .clone()
        .output(output_from_callback(callback, user_data));
    let Some(ds) = data_source_for_blob(&settings, blob) else {
        set_last_error(
            ctx,
            "hegel_test_case_from_blob: the supplied failure blob could not be decoded. \
             It may be corrupt or from an incompatible Hegel version.",
        );
        return HEGEL_E_INVALID_ARG;
    };
    let tc = handle_from_family(new_family(ds));
    unsafe { *out_test_case = tc };
    HEGEL_OK
}

/// Parameters:
/// `tc`: Any test-case handle. Safe to call with NULL.
///
/// Returns `HEGEL_OK`.
///
/// Each handle holds one reference to the shared test case. The underlying
/// data source is released once the last reference is gone. Each handle
/// must be freed exactly once. A run-owned test case still needs
/// `hegel_mark_complete` from one of its handles before the run can
/// advance, so make every test case complete before freeing your last
/// handle to it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_free(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    if tc.is_null() {
        return HEGEL_OK;
    }
    // SAFETY: `tc` is a non-null handle from a `hegel_*` constructor that the
    // caller is freeing exactly once; reconstituting the `Box` drops this
    // handle and its reference to the family.
    drop(unsafe { Box::from_raw(tc) });
    HEGEL_OK
}

/// Returns whether this test case belongs to a run already known to be
/// nondeterministic.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_is_nondeterministic(
    ctx: *mut HegelContext,
    tc: *const HegelTestCase,
    out_is_nondeterministic: *mut bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_test_case_is_nondeterministic", tc) } {
        Ok(pair) => pair,
        Err(rc) => return rc,
    };
    if out_is_nondeterministic.is_null() {
        set_last_error(
            ctx,
            "hegel_test_case_is_nondeterministic: out parameter is null",
        );
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_is_nondeterministic = tc.stream.is_nondeterministic() };
    HEGEL_OK
}

/// Parameters:
/// `out_test_case`: Receives a new handle onto an independent stream of
///   the same test case.
///
/// Returns `HEGEL_OK`, `HEGEL_E_CONCURRENT_USE` if another thread is
/// mid-operation on the source handle, `HEGEL_E_ALREADY_COMPLETE` once the
/// test case has completed.
///
/// The clone shares the test case's outcome and budgets but generates from
/// its own choice sequence, so a clone and its source can be driven
/// concurrently from different threads while staying deterministic under
/// replay. Collections, pools, and state machines remain shared across all
/// handles to the test case, but do not use shared objects from two streams
/// since it makes tests flaky (and a *collection* used from two threads at
/// once reports `HEGEL_E_CONCURRENT_USE`; see `hegel_collection_t`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_test_case_clone(
    ctx: *mut HegelContext,
    tc: *const HegelTestCase,
    out_test_case: *mut *mut HegelTestCase,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (src, _guard) = match unsafe { tc_guard(ctx, "hegel_test_case_clone", tc) } {
        Ok(pair) => pair,
        Err(rc) => return rc,
    };
    if out_test_case.is_null() {
        set_last_error(ctx, "hegel_test_case_clone: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_test_case = ptr::null_mut() };
    let stream = match src.stream.clone_stream() {
        Ok(stream) => stream,
        Err(e) => return translate_ds_error(ctx, e),
    };
    let clone = handle_from_stream(Arc::clone(&src.family), Arc::from(stream));
    unsafe { *out_test_case = clone };
    HEGEL_OK
}

/// Allocate a fresh family from a data source.
fn new_family(ds: Box<dyn DataSource + Send + Sync>) -> Arc<FamilyShared> {
    Arc::new(FamilyShared {
        ds: Arc::from(ds),
        completed: AtomicBool::new(false),
    })
}

/// Allocate the root handle for `family` — drawing from the family's root
/// stream — and return its raw pointer.
fn handle_from_family(family: Arc<FamilyShared>) -> *mut HegelTestCase {
    let stream = Arc::clone(&family.ds);
    handle_from_stream(family, stream)
}

/// Allocate a handle holding one reference to `family` that draws from
/// `stream`, and return its raw pointer. Each handle has its own `local`
/// buffer so concurrent handles do not stomp each other's borrowed values.
fn handle_from_stream(
    family: Arc<FamilyShared>,
    stream: Arc<dyn DataSource + Send + Sync>,
) -> *mut HegelTestCase {
    into_raw_send_sync(HegelTestCase {
        family,
        stream,
        local: Mutex::new(LocalState { completed: false }),
    })
}

/// Resolve a test-case handle for a per-test-case primitive, returning the
/// handle and its locked per-instance state.
///
/// Takes a *shared* reference (never `&mut`: two threads racing the same
/// handle pointer would make `&mut` instant UB, whereas `&HegelTestCase` is
/// sound because the type is `Sync`). Errors, in order, each recording a
/// `"<fn_name>: ..."` diagnostic on `ctx`:
/// - `HEGEL_E_INVALID_HANDLE` for a null pointer,
/// - `HEGEL_E_ALREADY_COMPLETE` if the family is already complete (checked
///   before the lock so completion wins over contention),
/// - `HEGEL_E_CONCURRENT_USE` if this handle is already locked by another
///   thread (each handle may be driven by at most one thread at a time).
unsafe fn tc_guard<'a>(
    ctx: *mut HegelContext,
    fn_name: &str,
    tc: *const HegelTestCase,
) -> Result<(&'a HegelTestCase, MutexGuard<'a, LocalState>), hegel_result_t> {
    let Some(tc) = (unsafe { tc.as_ref() }) else {
        set_last_error(ctx, &format!("{fn_name}: test case pointer is null"));
        return Err(HEGEL_E_INVALID_HANDLE);
    };
    if tc.family.completed.load(Ordering::Acquire) {
        set_last_error(ctx, &format!("{fn_name}: test case is already complete"));
        return Err(HEGEL_E_ALREADY_COMPLETE);
    }
    let Some(guard) = tc.local.try_lock() else {
        set_last_error(
            ctx,
            &format!("{fn_name}: test case handle is in use on another thread"),
        );
        return Err(HEGEL_E_CONCURRENT_USE);
    };
    Ok((tc, guard))
}

/// Like [`tc_guard`] but for `hegel_mark_complete`: no family-completion
/// check (completing must work on an already-complete family — a second clone
/// completing it is a no-op — so `hegel_mark_complete` does its own per-handle
/// and `compare_exchange` checks), and a *blocking* lock instead of
/// `try_lock`. Completion is first-caller-wins and always succeeds, so an
/// in-flight operation on the same handle is waited for rather than reported
/// as `HEGEL_E_CONCURRENT_USE`. Returns `HEGEL_E_INVALID_HANDLE` for a null
/// pointer.
unsafe fn tc_lock<'a>(
    ctx: *mut HegelContext,
    fn_name: &str,
    tc: *const HegelTestCase,
) -> Result<(&'a HegelTestCase, MutexGuard<'a, LocalState>), hegel_result_t> {
    let Some(tc) = (unsafe { tc.as_ref() }) else {
        set_last_error(ctx, &format!("{fn_name}: test case pointer is null"));
        return Err(HEGEL_E_INVALID_HANDLE);
    };
    Ok((tc, tc.local.lock()))
}

fn translate_ds_error(ctx: *mut HegelContext, e: DataSourceError) -> hegel_result_t {
    match e {
        DataSourceError::StopTest => HEGEL_E_STOP_TEST,
        DataSourceError::Assume => HEGEL_E_ASSUME,
        DataSourceError::InvalidArgument(msg) => {
            set_last_error(ctx, &msg);
            HEGEL_E_INVALID_ARG
        }
        DataSourceError::Internal(e) => {
            set_last_error(ctx, &e.to_string());
            HEGEL_E_INTERNAL
        }
    }
}

/// Reconstruct and drop an engine-allocated buffer handed out by a
/// `hegel_generate_*` draw. `data` must come from `Box::into_raw` on a boxed
/// `[u8]` of length `len` and must not be freed again.
unsafe fn free_engine_buffer(data: *mut u8, len: usize) {
    drop(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(data, len)) });
}

/// Shared prologue/epilogue for the typed `hegel_generate_*` draws: clear
/// the error channel, check the test-case handle, require a non-null out
/// pointer (reporting "<fn_name>: out parameter is null"), run `draw`
/// against the handle, pass the drawn value to `write`, and translate draw
/// errors onto `ctx`. `write` performs the caller's raw out-pointer store,
/// so it runs only when the out pointer is non-null and the draw succeeded.
unsafe fn typed_draw<T>(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    fn_name: &str,
    out_is_null: bool,
    draw: impl FnOnce(&HegelTestCase) -> Result<T, DataSourceError>,
    write: impl FnOnce(T),
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, fn_name, tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_is_null {
        set_last_error(ctx, &format!("{fn_name}: out parameter is null"));
        return HEGEL_E_INVALID_ARG;
    }
    match draw(tc) {
        Ok(v) => {
            write(v);
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// A span groups a set of draws so the shrinker can treat them as a unit.
/// Libraries should wrap each compound generator in a span.
///
/// Parameters:
/// `label`: Identifies what kind of structure this span groups. The
///   values reserved by libhegel are the `hegel_label_t` constants in
///   `hegel.h`. Libraries may use any stable `u64` to define their own
///   spans.
///
/// Returns `HEGEL_OK`.
///
/// Pair with exactly one `hegel_stop_span` call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_start_span(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    label: u64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_start_span", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.stream.start_span(label) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `discard`: Pass `true` to mark the span rejected (e.g. a `filter`
///   predicate didn't hold) so libhegel retries from before the span
///   opened.
///
/// Returns `HEGEL_OK`.
///
/// Closes the most recently opened span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_stop_span(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    discard: bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_stop_span", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.stream.stop_span(discard) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Opaque handle to an engine-managed variable-length collection.
///
/// Created by `hegel_new_collection` on a test case; driven by
/// `hegel_collection_more` / `hegel_collection_reject` through any handle of
/// the *same* test-case family (the root or any clone) — the continue/stop
/// decisions are drawn from whichever handle makes the call. A collection
/// must not be used from two threads at once: the operations take an
/// internal non-blocking lock and return `HEGEL_E_CONCURRENT_USE` on
/// contention.
///
/// The handle is independent of the test case and run it was created under:
/// free it with `hegel_collection_free` exactly once, at any point — before
/// or after the test case or run is freed, in any order relative to other
/// frees.
pub struct HegelCollection {
    state: Mutex<crate::native::core::ManyState>,
}

/// Resolve a collection handle, recording a diagnostic and returning
/// `HEGEL_E_INVALID_HANDLE` on a null pointer.
unsafe fn collection_ref<'a>(
    ctx: *mut HegelContext,
    fn_name: &str,
    collection: *const HegelCollection,
) -> Result<&'a HegelCollection, hegel_result_t> {
    match unsafe { collection.as_ref() } {
        Some(c) => Ok(c),
        None => {
            set_last_error(ctx, &format!("{fn_name}: collection handle is null"));
            Err(HEGEL_E_INVALID_HANDLE)
        }
    }
}

/// Lock a collection's sizing state for one operation. A collection may be
/// driven by at most one thread at a time, so contention is reported as
/// `HEGEL_E_CONCURRENT_USE` rather than waited out.
fn collection_lock<'a>(
    ctx: *mut HegelContext,
    fn_name: &str,
    collection: &'a HegelCollection,
) -> Result<MutexGuard<'a, crate::native::core::ManyState>, hegel_result_t> {
    match collection.state.try_lock() {
        Some(guard) => Ok(guard),
        None => {
            set_last_error(
                ctx,
                &format!("{fn_name}: collection handle is in use on another thread"),
            );
            Err(HEGEL_E_CONCURRENT_USE)
        }
    }
}

/// For variable-length values, libhegel decides how many elements to
/// produce. The caller loops on `hegel_collection_more`, drawing one
/// element per returned `true`.
///
/// Parameters:
/// `min_size` / `max_size`: Inclusive size bounds. Pass `UINT64_MAX` as
///   `max_size` for no upper bound.
/// `out_collection`: Receives a caller-owned handle to pass to the calls
///   below (through any handle of the same test-case family). Release it
///   with `hegel_collection_free` exactly once.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_collection(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_size: u64,
    max_size: u64,
    out_collection: *mut *mut HegelCollection,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_new_collection", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_collection.is_null() {
        set_last_error(ctx, "hegel_new_collection: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_collection = ptr::null_mut() };
    let max = if max_size == u64::MAX {
        None
    } else {
        Some(max_size)
    };
    if let Some(max) = max {
        if min_size > max {
            set_last_error(
                ctx,
                &format!(
                    "hegel_new_collection requires min_size <= max_size, \
                     got [{min_size}, {max}]"
                ),
            );
            return HEGEL_E_INVALID_ARG;
        }
    }
    match tc.stream.new_collection(min_size, max) {
        Ok(state) => {
            unsafe {
                *out_collection = into_raw_send_sync(HegelCollection {
                    state: Mutex::new(state),
                });
            }
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `out_more`: Receives whether libhegel wants another element, drawn from
///   `tc`'s stream. Call in a loop until it is `false` and draw the next
///   element in each loop iteration.
///
/// Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_CONCURRENT_USE`
/// when another thread is mid-operation on the collection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_more(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    collection: *mut HegelCollection,
    out_more: *mut bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_collection_more", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let collection = match unsafe { collection_ref(ctx, "hegel_collection_more", collection) } {
        Ok(c) => c,
        Err(rc) => return rc,
    };
    if out_more.is_null() {
        set_last_error(ctx, "hegel_collection_more: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let mut state = match collection_lock(ctx, "hegel_collection_more", collection) {
        Ok(state) => state,
        Err(rc) => return rc,
    };
    match tc.stream.collection_more(&mut state) {
        Ok(m) => {
            unsafe { *out_more = m };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `why`: Optional human-readable rejection reason (NULL is allowed).
///   Validated but currently unused, reserved for future rejection
///   diagnostics.
///
/// Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_CONCURRENT_USE`
/// when another thread is mid-operation on the collection.
///
/// Tells libhegel the last element it produced is invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_reject(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    collection: *mut HegelCollection,
    why: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_collection_reject", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let collection = match unsafe { collection_ref(ctx, "hegel_collection_reject", collection) } {
        Ok(c) => c,
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
    let mut state = match collection_lock(ctx, "hegel_collection_reject", collection) {
        Ok(state) => state,
        Err(rc) => return rc,
    };
    match tc.stream.collection_reject(&mut state, why_str) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Release a collection handle from `hegel_new_collection`. Safe to call
/// with NULL (a no-op that returns `HEGEL_OK`), and safe at any point in any
/// order relative to freeing the test case or the run. Each handle must be
/// freed exactly once; freeing the same handle twice is undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_free(
    ctx: *mut HegelContext,
    collection: *mut HegelCollection,
) -> hegel_result_t {
    clear_last_error(ctx);
    if !collection.is_null() {
        // SAFETY: `collection` came from `hegel_new_collection`'s
        // Box::into_raw and is freed exactly once here.
        drop(unsafe { Box::from_raw(collection) });
    }
    HEGEL_OK
}

/// Opaque handle to an engine-managed *recursive generation scope*: the
/// leaf budget and retry bookkeeping for one draw of a recursively defined
/// value (a tree, a document, ...).
///
/// Created by `hegel_new_recursion` on a test case, once per recursive
/// value drawn; driven by `hegel_recursion_branch` / `hegel_recursion_leaf`
/// / `hegel_recursion_retry` through any handle of the *same* test-case
/// family (the root or any clone) — decisions are drawn from whichever
/// handle makes the call. Like a pool, the scope holds an internal lock, so
/// clone handles driven from parallel threads share the leaf budget safely.
///
/// The protocol, for one sub-value (starting with the root at depth 0):
/// call `hegel_recursion_branch`; on `true` invoke the user's branch
/// function, drawing each of its sub-values at `depth + 1` with this same
/// protocol; on `false` call `hegel_recursion_leaf` and then draw one leaf.
/// When `hegel_recursion_leaf` returns `HEGEL_E_RETRY` the attempt has
/// outgrown the leaf budget: unwind out of the user's generators without
/// drawing anything further, call `hegel_recursion_retry`, and on `HEGEL_OK`
/// start the whole value again from the root. All policy — the branch
/// probabilities, the depth and leaf limits, and when to give up — lives in
/// the engine, so recursive values are identically distributed in every
/// language frontend.
///
/// The handle is independent of the test case and run it was created under:
/// free it with `hegel_recursion_free` exactly once, at any point — before
/// or after the test case or run is freed, in any order relative to other
/// frees.
pub struct HegelRecursion {
    state: Mutex<crate::native::core::RecursionState>,
}

/// Resolve a recursion handle, recording a diagnostic and returning
/// `HEGEL_E_INVALID_HANDLE` on a null pointer.
unsafe fn recursion_ref<'a>(
    ctx: *mut HegelContext,
    fn_name: &str,
    recursion: *const HegelRecursion,
) -> Result<&'a HegelRecursion, hegel_result_t> {
    match unsafe { recursion.as_ref() } {
        Some(r) => Ok(r),
        None => {
            set_last_error(ctx, &format!("{fn_name}: recursion handle is null"));
            Err(HEGEL_E_INVALID_HANDLE)
        }
    }
}

/// Open a recursive generation scope: libhegel decides where the value
/// branches, where it bottoms out in leaves, and when an attempt has grown
/// too large and must be retried. See `hegel_recursion_t` for the protocol.
///
/// Parameters:
/// `max_depth`: Branches nest at most this deep; sub-values at this depth
///   are always leaves, so 0 generates only leaves.
/// `max_leaves`: The most leaves one generated value may contain. Attempts
///   that outgrow it are discarded and retried with a lower branching
///   probability, and the test case is rejected as invalid when several
///   attempts in a row fail to fit.
/// `out_recursion`: Receives a caller-owned handle to pass to the calls
///   below (through any handle of the same test-case family). Release it
///   with `hegel_recursion_free` exactly once.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_recursion(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    max_depth: u64,
    max_leaves: u64,
    out_recursion: *mut *mut HegelRecursion,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_new_recursion", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_recursion.is_null() {
        set_last_error(ctx, "hegel_new_recursion: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_recursion = ptr::null_mut() };
    match tc.stream.new_recursion(max_depth, max_leaves) {
        Ok(state) => {
            unsafe {
                *out_recursion = into_raw_send_sync(HegelRecursion {
                    state: Mutex::new(state),
                });
            }
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `depth`: The nesting depth of the sub-value about to be drawn: 0 for the
///   root, and one more than the enclosing branch for its sub-values.
/// `out_branch`: Receives the leaf-or-branch decision, drawn from `tc`'s
///   stream: `true` means invoke the branch function, `false` means the
///   sub-value is a leaf (call `hegel_recursion_leaf`, then draw it).
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_recursion_branch(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    recursion: *mut HegelRecursion,
    depth: u64,
    out_branch: *mut bool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_recursion_branch", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let recursion = match unsafe { recursion_ref(ctx, "hegel_recursion_branch", recursion) } {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    if out_branch.is_null() {
        set_last_error(ctx, "hegel_recursion_branch: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let state = recursion.state.lock();
    match tc.stream.recursion_branch(&state, depth) {
        Ok(b) => {
            unsafe { *out_branch = b };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Count one leaf against the current attempt's budget. Call immediately
/// before drawing each leaf value.
///
/// Returns `HEGEL_OK` (draw the leaf), `HEGEL_E_RETRY` (the attempt has
/// outgrown `max_leaves`: unwind it without drawing anything further and
/// call `hegel_recursion_retry`), or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_recursion_leaf(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    recursion: *mut HegelRecursion,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_recursion_leaf", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let recursion = match unsafe { recursion_ref(ctx, "hegel_recursion_leaf", recursion) } {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    let mut state = recursion.state.lock();
    match tc.stream.recursion_leaf(&mut state) {
        Ok(true) => HEGEL_OK,
        Ok(false) => {
            set_last_error(
                ctx,
                &format!(
                    "recursive value needs more than max_leaves = {} leaves; \
                     discard the attempt with hegel_recursion_retry",
                    state.max_leaves
                ),
            );
            HEGEL_E_RETRY
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Discard a generation attempt that returned `HEGEL_E_RETRY`: the spans it
/// left open are closed and marked discarded, its leaf budget is reset, and
/// the next attempt uses a lower branching probability. Call only after
/// unwinding out of the user's generators, from the stack depth at which
/// `hegel_new_recursion` was called.
///
/// Returns `HEGEL_OK` (start the value again from the root),
/// `HEGEL_E_ASSUME` (attempts exhausted: the test case has been concluded
/// invalid, abort the body as for any failed assumption), or
/// `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_recursion_retry(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    recursion: *mut HegelRecursion,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_recursion_retry", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let recursion = match unsafe { recursion_ref(ctx, "hegel_recursion_retry", recursion) } {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    let mut state = recursion.state.lock();
    match tc.stream.recursion_retry(&mut state) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Release a recursion handle from `hegel_new_recursion`. Safe to call with
/// NULL (a no-op that returns `HEGEL_OK`), and safe at any point in any
/// order relative to freeing the test case or the run. Each handle must be
/// freed exactly once; freeing the same handle twice is undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_recursion_free(
    ctx: *mut HegelContext,
    recursion: *mut HegelRecursion,
) -> hegel_result_t {
    clear_last_error(ctx);
    if !recursion.is_null() {
        // SAFETY: `recursion` came from `hegel_new_recursion`'s
        // Box::into_raw and is freed exactly once here.
        drop(unsafe { Box::from_raw(recursion) });
    }
    HEGEL_OK
}

/// Opaque handle to an engine-managed *variable pool* for stateful testing.
///
/// Created by `hegel_new_pool` on a test case; driven by `hegel_pool_add` /
/// `hegel_pool_generate` through any handle of the *same* test-case family
/// (the root or any clone) — the draw comes from whichever handle makes the
/// call. Unlike a collection, a pool may legitimately be shared between
/// clone handles driven from parallel threads: it holds an internal lock,
/// so concurrent operations serialize instead of erroring. (Which variable
/// a concurrent draw picks then depends on scheduling order, with the usual
/// replay caveat for racy tests.)
///
/// The handle is independent of the test case and run it was created under:
/// free it with `hegel_pool_free` exactly once, at any point — before or
/// after the test case or run is freed, in any order relative to other
/// frees.
pub struct HegelPool {
    variables: Mutex<crate::native::core::NativeVariables>,
}

/// Resolve a pool handle, recording a diagnostic and returning
/// `HEGEL_E_INVALID_HANDLE` on a null pointer.
unsafe fn pool_ref<'a>(
    ctx: *mut HegelContext,
    fn_name: &str,
    pool: *const HegelPool,
) -> Result<&'a HegelPool, hegel_result_t> {
    match unsafe { pool.as_ref() } {
        Some(p) => Ok(p),
        None => {
            set_last_error(ctx, &format!("{fn_name}: pool handle is null"));
            Err(HEGEL_E_INVALID_HANDLE)
        }
    }
}

/// A pool tracks a set of variable ids libhegel can draw from and shrink
/// over. It is mostly used for stateful testing, where a rule needs to act
/// on some previously generated value. The caller keeps its own mapping
/// from variable id to the value it generated.
///
/// Parameters:
/// `out_pool`: Receives a caller-owned handle to pass to the calls below
///   (through any handle of the same test-case family). Release it with
///   `hegel_pool_free` exactly once.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_pool(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    out_pool: *mut *mut HegelPool,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_new_pool", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_pool.is_null() {
        set_last_error(ctx, "hegel_new_pool: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_pool = ptr::null_mut() };
    match tc.stream.new_pool() {
        Ok(variables) => {
            unsafe {
                *out_pool = into_raw_send_sync(HegelPool {
                    variables: Mutex::new(variables),
                });
            }
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `out_variable_id`: Receives a fresh variable id, which the caller
///   associates with the value it just generated.
///
/// The id is drawn from `tc`'s stream and recorded in the choice sequence
/// by value, so it stays stable while the test case shrinks: deleting an
/// earlier addition never renumbers the survivors.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_pool_add(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    pool: *mut HegelPool,
    out_variable_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_pool_add", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let pool = match unsafe { pool_ref(ctx, "hegel_pool_add", pool) } {
        Ok(p) => p,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error(ctx, "hegel_pool_add: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let mut variables = pool.variables.lock();
    match tc.stream.pool_add(&mut variables) {
        Ok(id) => {
            unsafe { *out_variable_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Draws a variable from the pool, letting libhegel choose (and shrink)
/// which previously added variable to reuse. The choice is drawn from
/// `tc`'s stream and recorded as the chosen variable id itself, not as an
/// index into the pool's current contents, so shrinking away other
/// additions never changes which variable a recorded choice refers to.
///
/// Parameters:
/// `consume`: When `true` the drawn variable is removed from the pool.
///   When `false` it is not removed.
/// `out_variable_id`: Receives the variable id libhegel chose.
///
/// Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_ASSUME` if the pool
/// has no variables — treat it like any other failed assumption.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_pool_generate(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    pool: *mut HegelPool,
    consume: bool,
    out_variable_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_pool_generate", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let pool = match unsafe { pool_ref(ctx, "hegel_pool_generate", pool) } {
        Ok(p) => p,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error(ctx, "hegel_pool_generate: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let mut variables = pool.variables.lock();
    match tc.stream.pool_generate(&mut variables, consume) {
        Ok(id) => {
            unsafe { *out_variable_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Release a pool handle from `hegel_new_pool`. Safe to call with NULL (a
/// no-op that returns `HEGEL_OK`), and safe at any point in any order
/// relative to freeing the test case or the run, provided no pool operation
/// is still in flight on another thread. Each handle must be freed exactly
/// once; freeing the same handle twice is undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_pool_free(
    ctx: *mut HegelContext,
    pool: *mut HegelPool,
) -> hegel_result_t {
    clear_last_error(ctx);
    if !pool.is_null() {
        // SAFETY: `pool` came from `hegel_new_pool`'s Box::into_raw and is
        // freed exactly once here.
        drop(unsafe { Box::from_raw(pool) });
    }
    HEGEL_OK
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
        unsafe { core::slice::from_raw_parts(names, len) }
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

/// Opaque handle to an engine-owned *state machine* for stateful
/// (rule-based) testing, sequential or concurrent.
///
/// Created by `hegel_new_state_machine` on a test case; driven by
/// `hegel_state_machine_next_group` / `hegel_state_machine_next_rule` /
/// `hegel_state_machine_rule_rejected` through any handle of the *same*
/// test-case family (the root or any clone) — each choice is drawn from
/// whichever handle makes the call. The machine holds an internal lock, so
/// concurrent use from two clone handles serializes instead of erroring.
///
/// The handle is independent of the test case and run it was created under:
/// free it with `hegel_state_machine_free` exactly once, at any point —
/// before or after the test case or run is freed, in any order relative to
/// other frees.
pub struct HegelStateMachine {
    machine: Mutex<crate::native::core::NativeStateMachine>,
}

/// Resolve a state-machine handle, recording a diagnostic and returning
/// `HEGEL_E_INVALID_HANDLE` on a null pointer.
unsafe fn state_machine_ref<'a>(
    ctx: *mut HegelContext,
    fn_name: &str,
    state_machine: *const HegelStateMachine,
) -> Result<&'a HegelStateMachine, hegel_result_t> {
    match unsafe { state_machine.as_ref() } {
        Some(m) => Ok(m),
        None => {
            set_last_error(ctx, &format!("{fn_name}: state machine handle is null"));
            Err(HEGEL_E_INVALID_HANDLE)
        }
    }
}

/// Register a *state machine* for engine-owned stateful (rule-based)
/// testing, sequential or concurrent: `num_rules` rules — each assigned to
/// a concurrency group by `rule_groups`, an array of group ids parallel to
/// `rule_names` — and `num_invariants` invariants, with names as
/// NUL-terminated UTF-8, plus concurrency bounds. Group ids are arbitrary
/// (any value except `HEGEL_STATE_MACHINE_DONE`, which
/// `hegel_state_machine_next_group` reserves as its termination sentinel):
/// the machine has one concurrency group per distinct value of
/// `rule_groups`. The engine draws the machine's concurrency
/// level — the number of workers (typically worker threads) that will pull
/// rules — in `[min_concurrency, max_concurrency]` and writes it into
/// `*out_concurrency`; the caller must run exactly that many workers. The
/// engine owns the distribution, which is weighted toward
/// `max_concurrency` (concurrency bugs need concurrency) rather than
/// shrink-biased toward the minimum. Pass `min_concurrency ==
/// max_concurrency` to fix the level without consuming entropy — `1, 1`
/// for a sequential machine.
///
/// The engine owns rule selection — including swarm testing, where each
/// worker enables a random subset of rules (at least one per group) and
/// selection draws only from that subset. The caller drives execution in
/// rounds: on the root test-case handle it asks
/// `hegel_state_machine_next_group` whether another round should run, then
/// each worker asks `hegel_state_machine_next_rule` which rule to run and
/// applies it, until that call signals the join point. Rules in
/// the same group may run concurrently; rules in different groups never
/// overlap.
///
/// Creating the machine draws from the calling handle's stream: the
/// concurrency level and each worker's swarm parameters are decided here,
/// up front, so the machine is fully constructed before any rule is
/// requested.
///
/// Creating a machine with `max_concurrency > 1` declares the run
/// nondeterministic: thread scheduling is outside the engine's control, so
/// nothing that assumes deterministic replay can be trusted. On a run not
/// already known to be nondeterministic, the first such creation is
/// rejected with `HEGEL_E_ASSUME` — the caller should abandon the body and
/// report the case `HEGEL_STATUS_INVALID`, exactly as for a failed
/// assumption — and the engine flips the run at that case's end. Every
/// later test case is marked nondeterministic before it starts (so a
/// frontend can capture its whole trace for the failure report, including
/// draws made before the machine is created) and its creations succeed.
/// From the flip on, the run reports failures faithfully from the
/// discovering execution and skips data-tree recording (and with it
/// novel-prefix generation and the nondeterminism mismatch check), span
/// mutation, the verify and shrink pass (and with it the flakiness check —
/// generation stops at the first bug, so at most one failure is reported),
/// targeting, and database persistence and reuse. Failures from such a run
/// carry no reproduce blob. A notice explaining this is printed once, on
/// the run's output, unless verbosity is quiet. This applies even to test
/// cases whose drawn concurrency level is 1: the declared bound is what
/// counts. Standalone test cases — `hegel_test_case_from_blob` replays —
/// are never rejected.
///
/// On success writes a caller-owned handle into `*out_state_machine` —
/// pass it to subsequent `hegel_state_machine_next_group` /
/// `hegel_state_machine_next_rule` / `hegel_state_machine_rule_rejected`
/// calls (through any handle of the same test-case family) and release it
/// with `hegel_state_machine_free` exactly once — writes the drawn
/// concurrency level into `*out_concurrency`, and returns `HEGEL_OK`.
/// Returns `HEGEL_E_ASSUME` for the run's first `max_concurrency > 1`
/// creation (the caller should abort the body and call
/// `hegel_mark_complete` with `HEGEL_STATUS_INVALID`; see above). Returns
/// `HEGEL_E_STOP_TEST` when the engine's choice budget is
/// exhausted (the caller should abort the body and call
/// `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`). Returns
/// `HEGEL_E_INVALID_ARG` if `num_rules` is zero, an entry of `rule_groups`
/// is `HEGEL_STATE_MACHINE_DONE`, `min_concurrency < 1`,
/// `max_concurrency < min_concurrency`, or on null / non-UTF-8 names.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_new_state_machine(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    rule_names: *const *const c_char,
    rule_groups: *const i64,
    num_rules: usize,
    invariant_names: *const *const c_char,
    num_invariants: usize,
    min_concurrency: i64,
    max_concurrency: i64,
    out_state_machine: *mut *mut HegelStateMachine,
    out_concurrency: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_new_state_machine", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_state_machine.is_null() || out_concurrency.is_null() {
        set_last_error(ctx, "hegel_new_state_machine: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_state_machine = ptr::null_mut() };
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
    if rule_groups.is_null() && num_rules > 0 {
        set_last_error(ctx, "hegel_new_state_machine: rule_groups is null");
        return HEGEL_E_INVALID_ARG;
    }
    let rule_groups: Vec<i64> = if num_rules == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(rule_groups, num_rules) }.to_vec()
    };
    if let Some(rule) = rule_groups
        .iter()
        .position(|&id| id == HEGEL_STATE_MACHINE_DONE)
    {
        set_last_error(
            ctx,
            &format!(
                "hegel_new_state_machine: rule_groups[{rule}] is {HEGEL_STATE_MACHINE_DONE} \
                 (HEGEL_STATE_MACHINE_DONE), which is reserved as the termination sentinel \
                 of hegel_state_machine_next_group"
            ),
        );
        return HEGEL_E_INVALID_ARG;
    }
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
    match tc.stream.new_state_machine(
        rules,
        rule_groups,
        invariants,
        min_concurrency,
        max_concurrency,
    ) {
        Ok(machine) => {
            let concurrency = machine.concurrency();
            unsafe {
                *out_state_machine = into_raw_send_sync(HegelStateMachine {
                    machine: Mutex::new(machine),
                });
                *out_concurrency = concurrency;
            }
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Value written to `*out_rule_index` by `hegel_state_machine_next_rule`
/// when the calling worker's round budget is exhausted (stop running rules
/// and wait for the next group / join point), and to `*out_group_id` by
/// `hegel_state_machine_next_group` when the whole state machine is done
/// (run no further rounds).
pub const HEGEL_STATE_MACHINE_DONE: i64 = i64::MIN;

/// Start the machine's next round: make the per-round stop decision (a
/// recorded boolean draw with a small stop probability, bounded by the
/// `stateful_step_count` setting) and, if the test case continues, draw
/// which concurrency group is current for the round. Writes the current
/// group's id (its value in the creating `rule_groups`) into
/// `*out_group_id` when a new round has begun and the workers should pull
/// rules again — the id identifies the round's group, e.g. for trace
/// output — or `HEGEL_STATE_MACHINE_DONE` (`INT64_MIN`) to indicate
/// termination of the whole state machine. (`hegel_new_state_machine`
/// rejects `HEGEL_STATE_MACHINE_DONE` as a group id so it stays
/// unambiguous here.)
///
/// Call this on the root test-case handle (the handle used for
/// hegel_new_state_machine) at every join point — after each worker's
/// `hegel_state_machine_next_rule` stream is exhausted — including before the
/// first rule is requested. This applies to sequential machines too: the
/// frontend must advance the group when the rule stream is exhausted, even
/// though there is only a single group.
///
/// `state_machine` must be a handle returned by `hegel_new_state_machine`
/// on this test-case family. Returns `HEGEL_E_STOP_TEST` when the
/// engine's choice budget is exhausted (the caller should abort the body
/// and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_state_machine_next_group(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    state_machine: *mut HegelStateMachine,
    out_group_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_state_machine_next_group", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let state_machine =
        match unsafe { state_machine_ref(ctx, "hegel_state_machine_next_group", state_machine) } {
            Ok(m) => m,
            Err(rc) => return rc,
        };
    if out_group_id.is_null() {
        set_last_error(ctx, "hegel_state_machine_next_group: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let mut machine = state_machine.machine.lock();
    match tc.stream.state_machine_next_group(&mut machine) {
        Ok(Some(group)) => {
            unsafe { *out_group_id = group };
            HEGEL_OK
        }
        Ok(None) => {
            unsafe { *out_group_id = HEGEL_STATE_MACHINE_DONE };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Draw the index of the next rule for worker `worker_index` to run this
/// round, letting the engine choose the rule sequence. The returned index
/// is always a rule belonging to the current concurrency group (see
/// `hegel_state_machine_next_group`). Swarm testing is applied per worker:
/// a random subset of rules is enabled (at least one per group) on the
/// worker's first selection and selection is restricted to that subset for
/// the rest of the test case.
///
/// `tc` may be any handle of the machine's test-case family: the machine's
/// state is family-wide, and the handle only determines which choice
/// stream the selection draws land in. At concurrency 1, it's safe to use
/// the root handle for everything. At concurrency > 1, each worker should
/// draw from its own `hegel_test_case_clone` handle (a single handle may
/// be driven by at most one thread at a time), cloned once before the
/// first round and kept for the whole test case, while the root handle
/// stays with whoever drives `hegel_state_machine_next_group`.
///
/// `worker_index` identifies the calling worker and must satisfy
/// `0 <= worker_index < concurrency` (the level drawn at state-machine
/// creation and written to `*out_concurrency`);
/// an index rather than the handle identifies the worker because a single
/// OS thread could hold multiple test-case clones. Draws consult only
/// per-worker and per-clone state, so draws on one worker don't affect
/// draws on another.
///
/// Writes `HEGEL_STATE_MACHINE_DONE` (`INT64_MIN`) into `*out_rule_index`
/// when the worker's round budget is exhausted: stop running rules and wait
/// for the next group / join point.
///
/// `state_machine` must be a handle returned by `hegel_new_state_machine`
/// on this test-case family. Returns `HEGEL_E_STOP_TEST` when the engine's
/// choice budget is exhausted (the caller should abort the body and call
/// `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_state_machine_next_rule(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    state_machine: *mut HegelStateMachine,
    worker_index: i64,
    out_rule_index: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_state_machine_next_rule", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let state_machine =
        match unsafe { state_machine_ref(ctx, "hegel_state_machine_next_rule", state_machine) } {
            Ok(m) => m,
            Err(rc) => return rc,
        };
    if out_rule_index.is_null() {
        set_last_error(ctx, "hegel_state_machine_next_rule: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let mut machine = state_machine.machine.lock();
    match tc
        .stream
        .state_machine_next_rule(&mut machine, worker_index)
    {
        Ok(Some(index)) => {
            unsafe { *out_rule_index = index };
            HEGEL_OK
        }
        Ok(None) => {
            unsafe { *out_rule_index = HEGEL_STATE_MACHINE_DONE };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Report that the rule most recently returned by
/// `hegel_state_machine_next_rule` to worker `worker_index` was rejected:
/// an assumption failed before the rule completed, so it should not count
/// toward libhegel's budget for the test case. At concurrency 1 the
/// current round then does not count toward the step budget; at
/// concurrency > 1 the rule does not advance the worker's per-round
/// continue/stop decision, so the worker's next
/// `hegel_state_machine_next_rule` call retries the slot.
///
/// `worker_index` must satisfy `0 <= worker_index < concurrency`, exactly
/// as for `hegel_state_machine_next_rule`.
///
/// Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` when the worker has no
/// outstanding rule — no rule has been returned to it this round, its
/// current rule was already reported as rejected, or it has already pulled
/// another rule.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_state_machine_rule_rejected(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    state_machine: *mut HegelStateMachine,
    worker_index: i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_state_machine_rule_rejected", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let state_machine =
        match unsafe { state_machine_ref(ctx, "hegel_state_machine_rule_rejected", state_machine) }
        {
            Ok(m) => m,
            Err(rc) => return rc,
        };
    let mut machine = state_machine.machine.lock();
    match tc
        .stream
        .state_machine_rule_rejected(&mut machine, worker_index)
    {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Release a state-machine handle from `hegel_new_state_machine`. Safe to
/// call with NULL (a no-op that returns `HEGEL_OK`), and safe at any point
/// in any order relative to freeing the test case or the run. Each handle
/// must be freed exactly once; freeing the same handle twice is undefined
/// behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_state_machine_free(
    ctx: *mut HegelContext,
    state_machine: *mut HegelStateMachine,
) -> hegel_result_t {
    clear_last_error(ctx);
    if !state_machine.is_null() {
        // SAFETY: `state_machine` came from `hegel_new_state_machine`'s
        // Box::into_raw and is freed exactly once here.
        drop(unsafe { Box::from_raw(state_machine) });
    }
    HEGEL_OK
}

/// Parameters:
/// `p`: Probability of drawing `true`. Must be in `[0.0, 1.0]`; `p = 0.0`
///   always yields `false` and `p = 1.0` always yields `true` without
///   consuming entropy.
/// `forced` / `has_forced`: When `has_forced` is set, the result is
///   forced to `forced`.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_boolean(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    p: f64,
    forced: bool,
    has_forced: bool,
    out_value: *mut bool,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_boolean",
            out_value.is_null(),
            |tc| tc.stream.generate_boolean(p, has_forced.then_some(forced)),
            |v| *out_value = v,
        )
    }
}

/// Parameters:
/// `min_value` / `max_value`: Inclusive bounds. Both required.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_integer(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_value: i64,
    max_value: i64,
    out_value: *mut i64,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_integer",
            out_value.is_null(),
            |tc| {
                let v = tc
                    .stream
                    .generate_integer(&BigInt::from(min_value), &BigInt::from(max_value))?;
                let narrowed = i64::try_from(v).ok();
                Ok(hegel_internal_unwrap!(
                    narrowed,
                    "hegel_generate_integer: drawn value does not fit the requested i64 bounds"
                ))
            },
            |v| *out_value = v,
        )
    }
}

/// Parameters:
/// `min_value` / `max_value`: Inclusive bounds as two's-complement
///   little-endian signed byte buffers. Both required and must be
///   non-empty.
/// `out_value`: Receives the drawn value's two's-complement little-endian
///   bytes. libhegel sign-fills the rest of the buffer up to
///   `out_value_cap`, so reading the whole buffer as a fixed-width integer
///   also yields the drawn value with no sign extension needed.
/// `out_value_len`: Receives the value's minimal length. Passing
///   `out_value_cap >= max(min_value_len, max_value_len)` always succeeds.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
///
/// Use this for bounds outside the `int64_t` range; otherwise prefer
/// `hegel_generate_integer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_integer_big(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_value: *const u8,
    min_value_len: usize,
    max_value: *const u8,
    max_value_len: usize,
    out_value: *mut u8,
    out_value_cap: usize,
    out_value_len: *mut usize,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_generate_integer_big", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if min_value.is_null() {
        set_last_error(ctx, "hegel_generate_integer_big: min_value pointer is null");
        return HEGEL_E_INVALID_ARG;
    }
    if max_value.is_null() {
        set_last_error(ctx, "hegel_generate_integer_big: max_value pointer is null");
        return HEGEL_E_INVALID_ARG;
    }
    if min_value_len == 0 || max_value_len == 0 {
        set_last_error(
            ctx,
            "hegel_generate_integer_big: bound encodings must not be empty",
        );
        return HEGEL_E_INVALID_ARG;
    }
    if out_value.is_null() || out_value_len.is_null() {
        set_last_error(ctx, "hegel_generate_integer_big: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    let min_bytes = unsafe { core::slice::from_raw_parts(min_value, min_value_len) };
    let max_bytes = unsafe { core::slice::from_raw_parts(max_value, max_value_len) };
    let min = BigInt::from_signed_bytes_le(min_bytes);
    let max = BigInt::from_signed_bytes_le(max_bytes);
    match tc.stream.generate_integer(&min, &max) {
        Ok(v) => {
            let bytes = v.to_signed_bytes_le();
            if bytes.len() > out_value_cap {
                set_last_error(
                    ctx,
                    &format!(
                        "hegel_generate_integer_big: out buffer too small \
                         (need {}, have {})",
                        bytes.len(),
                        out_value_cap
                    ),
                );
                return HEGEL_E_INVALID_ARG;
            }
            let fill = if bytes.last().unwrap() & 0x80 != 0 {
                0xFF
            } else {
                0x00
            };
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), out_value, bytes.len());
                core::ptr::write_bytes(
                    out_value.add(bytes.len()),
                    fill,
                    out_value_cap - bytes.len(),
                );
                *out_value_len = bytes.len();
            }
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `width`: 32 or 64. 32 bit bounds must be exactly representable as
///   `float`, and finite 32 bit results are exactly representable as
///   `float`.
/// `min_value` / `max_value`: Inclusive bounds. Pass `-INFINITY` /
///   `INFINITY` for unbounded ends.
/// `allow_nan`: NaN is drawn only when this is set.
/// `allow_infinity`: Infinities are drawn only when this is set and the
///   corresponding endpoint is unbounded.
/// `exclude_min` / `exclude_max`: Make the corresponding bound exclusive
///   by stepping it to the next representable value at the requested width.
/// `smallest_nonzero_magnitude`: Nonzero magnitudes below this are never
///   drawn. Must be positive and finite; pass `5e-324` (width 64) or the
///   smallest `float` subnormal (width 32) for no restriction.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_float(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    width: u32,
    min_value: f64,
    max_value: f64,
    allow_nan: bool,
    allow_infinity: bool,
    exclude_min: bool,
    exclude_max: bool,
    smallest_nonzero_magnitude: f64,
    out_value: *mut f64,
) -> hegel_result_t {
    let spec = crate::native::draws::FloatSpec {
        width,
        min_value,
        max_value,
        allow_nan,
        allow_infinity,
        exclude_min,
        exclude_max,
        smallest_nonzero_magnitude,
    };
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_float",
            out_value.is_null(),
            |tc| tc.stream.generate_float(&spec),
            |v| *out_value = v,
        )
    }
}

/// An engine-allocated byte buffer returned by `hegel_generate_bytes`.
///
/// The caller owns the buffer and must release it with
/// `hegel_generate_bytes_result_free` (freeing through any other allocator
/// is undefined behaviour). `data` is never NULL after a successful draw,
/// even for `len == 0`.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct hegel_generate_bytes_result_t {
    pub data: *mut u8,
    pub len: usize,
}

/// Parameters:
/// `min_size` / `max_size`: Inclusive length bounds.
/// `out_result`: Receives a libhegel-allocated
///   `{uint8_t *data; size_t len;}` the caller owns. `data` is never NULL
///   after a successful draw. Release with
///   `hegel_generate_bytes_result_free`.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_bytes(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_size: u64,
    max_size: u64,
    out_result: *mut hegel_generate_bytes_result_t,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_bytes",
            out_result.is_null(),
            |tc| {
                tc.stream
                    .generate_bytes(size_arg(min_size), size_arg(max_size))
            },
            |v| {
                let boxed = v.into_boxed_slice();
                let len = boxed.len();
                let data = Box::into_raw(boxed) as *mut u8;
                *out_result = hegel_generate_bytes_result_t { data, len };
            },
        )
    }
}

/// Parameters:
/// `result`: Released and reset to `{NULL, 0}`. Safe to call with NULL or
///   an already-freed (zeroed) struct.
///
/// Returns `HEGEL_OK`.
///
/// Freeing the buffer any other way is undefined behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_bytes_result_free(
    ctx: *mut HegelContext,
    result: *mut hegel_generate_bytes_result_t,
) -> hegel_result_t {
    clear_last_error(ctx);
    let Some(result) = (unsafe { result.as_mut() }) else {
        return HEGEL_OK;
    };
    if !result.data.is_null() {
        // SAFETY: `data`/`len` came from `Box::into_raw` on a boxed slice in
        // `hegel_generate_bytes` and are freed exactly once here (the struct
        // is zeroed below, making a second call a no-op).
        unsafe { free_engine_buffer(result.data, result.len) };
    }
    result.data = ptr::null_mut();
    result.len = 0;
    HEGEL_OK
}

/// Specification of a string draw
///
/// Build one with a `hegel_string_generator_*` constructor (text, regex,
/// email, url, domain). Every parameter is validated at construction.
/// A generator is immutable after construction and may be shared freely
/// across test cases and threads. Free it with
/// `hegel_string_generator_free` once no draws will use it again.
pub struct HegelStringGenerator {
    spec: crate::native::draws::StringSpec,
}

/// Translate a constructor-time engine error onto `ctx`. Constructors
/// perform no draws, so any error they report is an invalid argument —
/// unless it is a violated internal invariant, which reports as
/// `HEGEL_E_INTERNAL`.
fn translate_construct_error(
    ctx: *mut HegelContext,
    e: crate::native::core::EngineError,
) -> hegel_result_t {
    set_last_error(ctx, &e.to_string());
    match e {
        crate::native::core::EngineError::Internal(_) => HEGEL_E_INTERNAL,
        _ => HEGEL_E_INVALID_ARG,
    }
}

/// Convert a `u64` size argument to `usize`, saturating on 32-bit targets
/// so an oversized request stays "absurdly large" (and fails at draw time
/// like any other unsatisfiable size) instead of silently truncating to a
/// small value.
fn size_arg(v: u64) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// Read an optional NUL-terminated UTF-8 string argument. `Err` carries the
/// invalid-argument diagnostic already set on `ctx`.
unsafe fn optional_utf8_arg(
    ctx: *mut HegelContext,
    fn_name: &str,
    arg_name: &str,
    p: *const c_char,
) -> Result<Option<String>, hegel_result_t> {
    if p.is_null() {
        return Ok(None);
    }
    match unsafe { CStr::from_ptr(p) }.to_str() {
        Ok(s) => Ok(Some(s.to_string())),
        Err(_) => {
            set_last_error(ctx, &format!("{fn_name}: {arg_name} is not valid UTF-8"));
            Err(HEGEL_E_INVALID_ARG)
        }
    }
}

/// Read an optional length-delimited UTF-8 buffer argument. A NULL pointer
/// means "absent". Length-delimited so the buffer may contain NUL bytes
/// (U+0000 is a valid character to include or exclude).
unsafe fn optional_utf8_buffer_arg(
    ctx: *mut HegelContext,
    fn_name: &str,
    arg_name: &str,
    p: *const u8,
    len: usize,
) -> Result<Option<String>, hegel_result_t> {
    if p.is_null() {
        return Ok(None);
    }
    let bytes = unsafe { core::slice::from_raw_parts(p, len) };
    match core::str::from_utf8(bytes) {
        Ok(s) => Ok(Some(s.to_string())),
        Err(_) => {
            set_last_error(ctx, &format!("{fn_name}: {arg_name} is not valid UTF-8"));
            Err(HEGEL_E_INVALID_ARG)
        }
    }
}

/// Read an optional array of NUL-terminated UTF-8 strings. A NULL array
/// means "absent"; a non-NULL array with `len == 0` means "present and
/// empty" (for `categories`, an empty alphabet).
unsafe fn optional_utf8_array_arg(
    ctx: *mut HegelContext,
    fn_name: &str,
    arg_name: &str,
    p: *const *const c_char,
    len: usize,
) -> Result<Option<Vec<String>>, hegel_result_t> {
    if p.is_null() {
        return Ok(None);
    }
    unsafe { names_from_c_array(ctx, fn_name, arg_name, p, len) }.map(Some)
}

/// Write a constructed string generator through `out_generator`, boxing it
/// into a caller-owned handle.
unsafe fn write_string_generator(
    out_generator: *mut *mut HegelStringGenerator,
    spec: crate::native::draws::StringSpec,
) -> hegel_result_t {
    let handle = into_raw_send_sync(HegelStringGenerator { spec });
    unsafe { *out_generator = handle };
    HEGEL_OK
}

/// Parameters:
/// `min_size` / `max_size`: Inclusive length bounds, in characters.
/// `codec`: The alphabet's starting range: `"ascii"`, `"latin-1"` /
///   `"iso-8859-1"`, or `"utf-8"` / NULL for Unicode.
/// `min_codepoint` / `max_codepoint`: Intersected with the codec's range.
///   Pass `0` and `UINT32_MAX` for no constraint. Surrogates are always
///   removed.
/// `categories`: Restricts to the union of the named Unicode general
///   categories. NULL means no restriction. A non-NULL empty list means an
///   empty alphabet.
/// `exclude_categories`: Removes the named categories.
/// `include_characters` / `exclude_characters`: UTF-8 buffers (pointer
///   plus byte length) of individual characters. Characters in
///   `include_characters` are included first, then characters in
///   `exclude_characters` are removed.
///
/// Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for constraints that leave
/// no characters while `max_size > 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_string_generator_text(
    ctx: *mut HegelContext,
    min_size: u64,
    max_size: u64,
    codec: *const c_char,
    min_codepoint: u32,
    max_codepoint: u32,
    categories: *const *const c_char,
    categories_len: usize,
    exclude_categories: *const *const c_char,
    exclude_categories_len: usize,
    include_characters: *const u8,
    include_characters_len: usize,
    exclude_characters: *const u8,
    exclude_characters_len: usize,
    out_generator: *mut *mut HegelStringGenerator,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_generator.is_null() {
        set_last_error(ctx, "hegel_string_generator_text: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_generator = ptr::null_mut() };
    const FN: &str = "hegel_string_generator_text";
    let codec = match unsafe { optional_utf8_arg(ctx, FN, "codec", codec) } {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let categories =
        match unsafe { optional_utf8_array_arg(ctx, FN, "categories", categories, categories_len) }
        {
            Ok(v) => v,
            Err(rc) => return rc,
        };
    let exclude_categories = match unsafe {
        optional_utf8_array_arg(
            ctx,
            FN,
            "exclude_categories",
            exclude_categories,
            exclude_categories_len,
        )
    } {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let include_characters = match unsafe {
        optional_utf8_buffer_arg(
            ctx,
            FN,
            "include_characters",
            include_characters,
            include_characters_len,
        )
    } {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let exclude_characters = match unsafe {
        optional_utf8_buffer_arg(
            ctx,
            FN,
            "exclude_characters",
            exclude_characters,
            exclude_characters_len,
        )
    } {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let alphabet = crate::native::draws::TextAlphabet {
        codec,
        min_codepoint,
        max_codepoint,
        categories,
        exclude_categories,
        include_characters,
        exclude_characters,
    };
    match crate::native::draws::StringSpec::text(&alphabet, size_arg(min_size), size_arg(max_size))
    {
        Ok(spec) => unsafe { write_string_generator(out_generator, spec) },
        Err(e) => translate_construct_error(ctx, e),
    }
}

/// Parameters:
/// `pattern`: The pattern to match, in Python `re` syntax.
/// `fullmatch`: When true, the whole string must match the pattern.
///   Otherwise, the match may be padded on either side.
/// `alphabet`: Optional (NULL for none). Must be a text generator. Its
///   character set constrains the padding and wildcard characters.
///
/// Returns `HEGEL_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_string_generator_regex(
    ctx: *mut HegelContext,
    pattern: *const c_char,
    fullmatch: bool,
    alphabet: *const HegelStringGenerator,
    out_generator: *mut *mut HegelStringGenerator,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_generator.is_null() {
        set_last_error(ctx, "hegel_string_generator_regex: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_generator = ptr::null_mut() };
    let pattern =
        match unsafe { optional_utf8_arg(ctx, "hegel_string_generator_regex", "pattern", pattern) }
        {
            Ok(Some(s)) => s,
            Ok(None) => {
                set_last_error(ctx, "hegel_string_generator_regex: pattern is null");
                return HEGEL_E_INVALID_ARG;
            }
            Err(rc) => return rc,
        };
    let alphabet_spec = unsafe { alphabet.as_ref() }.map(|g| &g.spec);
    match crate::native::draws::StringSpec::regex(&pattern, fullmatch, alphabet_spec) {
        Ok(spec) => unsafe { write_string_generator(out_generator, spec) },
        Err(e) => translate_construct_error(ctx, e),
    }
}

/// Returns `HEGEL_OK`. Produces RFC 5321/5322 addresses like
/// `alice@example.com`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_string_generator_email(
    ctx: *mut HegelContext,
    out_generator: *mut *mut HegelStringGenerator,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_generator.is_null() {
        set_last_error(ctx, "hegel_string_generator_email: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_generator = ptr::null_mut() };
    unsafe { write_string_generator(out_generator, crate::native::draws::StringSpec::email()) }
}

/// Returns `HEGEL_OK`. Produces RFC 3986 `http`/`https` URLs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_string_generator_url(
    ctx: *mut HegelContext,
    out_generator: *mut *mut HegelStringGenerator,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_generator.is_null() {
        set_last_error(ctx, "hegel_string_generator_url: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_generator = ptr::null_mut() };
    unsafe { write_string_generator(out_generator, crate::native::draws::StringSpec::url()) }
}

/// Parameters:
/// `max_length`: Total length of the fully-qualified domain name, in
///   `4..=255`.
///
/// Returns `HEGEL_OK`, or `HEGEL_E_INVALID_ARG` for a `max_length` that
/// leaves no eligible top-level domains.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_string_generator_domain(
    ctx: *mut HegelContext,
    max_length: u64,
    out_generator: *mut *mut HegelStringGenerator,
) -> hegel_result_t {
    clear_last_error(ctx);
    if out_generator.is_null() {
        set_last_error(ctx, "hegel_string_generator_domain: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    unsafe { *out_generator = ptr::null_mut() };
    match crate::native::draws::StringSpec::domain(size_arg(max_length)) {
        Ok(spec) => unsafe { write_string_generator(out_generator, spec) },
        Err(e) => translate_construct_error(ctx, e),
    }
}

/// Parameters:
/// `generator`: The generator to release. Safe to call with NULL.
///
/// Returns `HEGEL_OK`.
///
/// Each generator must be freed exactly once, and only after every draw
/// using it has completed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_string_generator_free(
    ctx: *mut HegelContext,
    generator: *mut HegelStringGenerator,
) -> hegel_result_t {
    clear_last_error(ctx);
    if generator.is_null() {
        return HEGEL_OK;
    }
    // SAFETY: `generator` came from `write_string_generator`'s Box::into_raw
    // and is freed exactly once here.
    drop(unsafe { Box::from_raw(generator) });
    HEGEL_OK
}

/// An engine-allocated string buffer returned by `hegel_generate_string`.
///
/// `data` points to `len` bytes of UTF-8. The buffer is **not**
/// NUL-terminated and may contain interior NUL bytes (the drawn alphabet
/// can include U+0000), so it is not a C string — always use `len`. The
/// caller owns the buffer and must release it with
/// `hegel_generate_string_result_free` (freeing through any other allocator
/// is undefined behaviour). `data` is never NULL after a successful draw,
/// even for `len == 0`.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct hegel_generate_string_result_t {
    pub data: *mut c_char,
    pub len: usize,
}

/// Parameters:
/// `generator`: A generator built by one of the constructors above.
/// `out_result`: Receives a libhegel-allocated
///   `{char *data; size_t len;}` the caller owns. Not NUL-terminated, and
///   it may contain interior NUL bytes since the drawn alphabet can include
///   U+0000, so always use `len`. Release with
///   `hegel_generate_string_result_free`.
///
/// Returns `HEGEL_OK`, `HEGEL_E_STOP_TEST`, or `HEGEL_E_ASSUME` when the
/// draw rejected itself (for example an email exceeding the RFC length
/// cap).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_string(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    generator: *const HegelStringGenerator,
    out_result: *mut hegel_generate_string_result_t,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_generate_string", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    let Some(generator) = (unsafe { generator.as_ref() }) else {
        set_last_error(ctx, "hegel_generate_string: generator handle is null");
        return HEGEL_E_INVALID_HANDLE;
    };
    if out_result.is_null() {
        set_last_error(ctx, "hegel_generate_string: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.stream.generate_string(&generator.spec) {
        Ok(s) => {
            let boxed = s.into_bytes().into_boxed_slice();
            let len = boxed.len();
            let data = Box::into_raw(boxed).cast::<c_char>();
            unsafe { *out_result = hegel_generate_string_result_t { data, len } };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `result`: Released and reset to `{NULL, 0}`. Safe to call with NULL or
///   an already-freed (zeroed) struct.
///
/// Returns `HEGEL_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_string_result_free(
    ctx: *mut HegelContext,
    result: *mut hegel_generate_string_result_t,
) -> hegel_result_t {
    clear_last_error(ctx);
    let Some(result) = (unsafe { result.as_mut() }) else {
        return HEGEL_OK;
    };
    if !result.data.is_null() {
        // SAFETY: `data`/`len` came from `Box::into_raw` on a boxed slice in
        // `hegel_generate_string` and are freed exactly once here (the
        // struct is zeroed below, making a second call a no-op).
        unsafe { free_engine_buffer(result.data.cast::<u8>(), result.len) };
    }
    result.data = ptr::null_mut();
    result.len = 0;
    HEGEL_OK
}

/// A drawn proleptic Gregorian calendar date: `year` in
/// `[-999999, 999999]` (bounded by the range passed to
/// `hegel_generate_date`), `month` in `[1, 12]`, `day` in
/// `[1, days-in-month]`.
#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
pub struct hegel_date_t {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// A drawn time of day: `hour` in `[0, 23]`, `minute` and `second` in
/// `[0, 59]`, `microsecond` in `[0, 999999]`.
#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
pub struct hegel_time_t {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub microsecond: u32,
}

/// A drawn naive datetime (a date plus a time of day, no timezone).
#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
pub struct hegel_datetime_t {
    pub date: hegel_date_t,
    pub time: hegel_time_t,
}

fn rust_date(d: &hegel_date_t) -> crate::native::draws::special::Date {
    crate::native::draws::special::Date {
        year: d.year,
        month: d.month,
        day: d.day,
    }
}

fn rust_time(t: &hegel_time_t) -> crate::native::draws::special::Time {
    crate::native::draws::special::Time {
        hour: t.hour,
        minute: t.minute,
        second: t.second,
        microsecond: t.microsecond,
    }
}

fn rust_datetime(dt: &hegel_datetime_t) -> crate::native::draws::special::DateTime {
    crate::native::draws::special::DateTime {
        date: rust_date(&dt.date),
        time: rust_time(&dt.time),
    }
}

fn c_date(d: crate::native::draws::special::Date) -> hegel_date_t {
    hegel_date_t {
        year: d.year,
        month: d.month,
        day: d.day,
    }
}

fn c_time(t: crate::native::draws::special::Time) -> hegel_time_t {
    hegel_time_t {
        hour: t.hour,
        minute: t.minute,
        second: t.second,
        microsecond: t.microsecond,
    }
}

/// Parameters:
/// `min_value` / `max_value`: Inclusive bounds, as proleptic Gregorian
///   dates with `year` in `[-999999, 999999]`. Pass `{1, 1, 1}` and
///   `{9999, 12, 31}` for the full range.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
///
/// Shrinks toward 2000-01-01 or the nearest bound when that is out of
/// range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_date(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_value: hegel_date_t,
    max_value: hegel_date_t,
    out_value: *mut hegel_date_t,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_date",
            out_value.is_null(),
            |tc| {
                tc.stream
                    .generate_date(rust_date(&min_value), rust_date(&max_value))
            },
            |d| *out_value = c_date(d),
        )
    }
}

/// Parameters:
/// `min_value` / `max_value`: Inclusive bounds. Pass all-zeros and
///   `{23, 59, 59, 999999}` for the full day.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
///
/// Shrinks toward `min_value`, the representable time closest to midnight.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_time(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_value: hegel_time_t,
    max_value: hegel_time_t,
    out_value: *mut hegel_time_t,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_time",
            out_value.is_null(),
            |tc| {
                tc.stream
                    .generate_time(rust_time(&min_value), rust_time(&max_value))
            },
            |t| *out_value = c_time(t),
        )
    }
}

/// Parameters:
/// `min_value` / `max_value`: Inclusive bounds on a naive datetime (no
///   timezone).
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
///
/// Shrinks toward 2000-01-01T00:00:00 or the nearest bound when that is out
/// of range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_datetime(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    min_value: hegel_datetime_t,
    max_value: hegel_datetime_t,
    out_value: *mut hegel_datetime_t,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_datetime",
            out_value.is_null(),
            |tc| {
                tc.stream
                    .generate_datetime(rust_datetime(&min_value), rust_datetime(&max_value))
            },
            |dt| {
                *out_value = hegel_datetime_t {
                    date: c_date(dt.date),
                    time: c_time(dt.time),
                }
            },
        )
    }
}

/// Parameters:
/// `version` / `has_version`: When `has_version` is set, the RFC 4122
///   version nibble is forced to `version` (0..=15, conventionally 1..=5)
///   and the variant nibble to the RFC 4122 variant. Without a version the
///   128 bits are uniform, except that the nil UUID is never produced.
/// `out_bytes`: Receives 16 big-endian bytes.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_uuid(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    version: u8,
    has_version: bool,
    out_bytes: *mut u8,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_uuid",
            out_bytes.is_null(),
            |tc| tc.stream.generate_uuid(has_version.then_some(version)),
            |bytes| core::ptr::copy_nonoverlapping(bytes.as_ptr(), out_bytes, 16),
        )
    }
}

/// Parameters:
/// `out_bytes`: Receives the address's 4 network-order bytes.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_ipv4(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    out_bytes: *mut u8,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_ipv4",
            out_bytes.is_null(),
            |tc| tc.stream.generate_ipv4(),
            |a| {
                let octets = a.octets();
                core::ptr::copy_nonoverlapping(octets.as_ptr(), out_bytes, 4);
            },
        )
    }
}

/// Parameters:
/// `out_bytes`: Receives the address's 16 network-order bytes.
///
/// Returns `HEGEL_OK` or `HEGEL_E_STOP_TEST`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_generate_ipv6(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    out_bytes: *mut u8,
) -> hegel_result_t {
    unsafe {
        typed_draw(
            ctx,
            tc,
            "hegel_generate_ipv6",
            out_bytes.is_null(),
            |tc| tc.stream.generate_ipv6(),
            |a| {
                let octets = a.octets();
                core::ptr::copy_nonoverlapping(octets.as_ptr(), out_bytes, 16);
            },
        )
    }
}

/// Parameters:
/// `value`: A numeric observation. Must be finite. Higher is "more
///   interesting." libhegel biases later test cases toward inputs that
///   produced higher observations under the same label.
/// `label`: Non-NULL, valid UTF-8. Each label may be recorded at most
///   once per test case.
///
/// Returns `HEGEL_OK`.
///
/// Has no effect unless `HEGEL_PHASE_TARGET` is enabled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_target(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    value: f64,
    label: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_target", tc) } {
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
    match tc.stream.target_observation(value, label) {
        Ok(()) => HEGEL_OK,
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Parameters:
/// `status`: A `hegel_status_t` value describing how the test case ended.
/// `origin`: Identifies the origin of a failure. Used only when `status`
///   is `HEGEL_STATUS_INTERESTING`; NULL otherwise.
///
/// Returns `HEGEL_OK`, or `HEGEL_E_ALREADY_COMPLETE` if called twice on the
/// same handle.
///
/// Completion is first-caller-wins and applies to the whole test case: the
/// first call from any handle records the outcome, and a later call on a
/// different handle is a safe no-op. This function never returns
/// `HEGEL_E_CONCURRENT_USE`: if another thread is mid-operation on the
/// handle it waits, then completes.
///
/// Choosing an origin string: libhegel groups failures by their `origin`. Two failures with identical
/// origins are the same bug and get shrunk together. Each new origin is a
/// new bug.
///
/// A library must pass a stable value for the origin, such as the location
/// of the failing assertion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_mark_complete(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    status: u32,
    origin: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, mut guard) = match unsafe { tc_lock(ctx, "hegel_mark_complete", tc) } {
        Ok(pair) => pair,
        Err(rc) => return rc,
    };

    // Completing the *same* handle twice is a usage error. (A different handle
    // in the family completing after this one is handled below: it is a no-op,
    // not an error.)
    if guard.completed {
        return HEGEL_E_ALREADY_COMPLETE;
    }

    let outcome = match status {
        x if x == hegel_status_t::HEGEL_STATUS_VALID as u32 => TestCaseResult::Valid,
        x if x == hegel_status_t::HEGEL_STATUS_INVALID as u32 => TestCaseResult::Invalid,
        x if x == hegel_status_t::HEGEL_STATUS_OVERRUN as u32 => TestCaseResult::Overrun,
        x if x == hegel_status_t::HEGEL_STATUS_INTERESTING as u32 => {
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
        _ => {
            set_last_error(
                ctx,
                &format!("hegel_mark_complete: unknown status {status}"),
            );
            return HEGEL_E_INVALID_ARG;
        }
    };

    guard.completed = true;

    // First handle in the family to complete wins: it records the outcome and
    // unblocks the run. A later clone completing the (already-complete) family
    // is a safe no-op, so concurrent clones don't race to an error.
    tc.family.complete(&outcome);
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

/// Parameters:
/// `out_status`: Receives `HEGEL_RUN_STATUS_PASSED`,
///   `HEGEL_RUN_STATUS_FAILED`, `HEGEL_RUN_STATUS_ERROR`, or
///   `HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC`.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `out_error`: Receives the run-level error message when the run
///   errored — a failed health check, a nondeterministic test, a violated
///   engine invariant — or NULL when it completed normally. Owned by the
///   run result and valid until `hegel_run_result_free`.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `out_count`: Receives the number of distinct failures, by origin, that
///   the run surfaced.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `index`: 0-based; must be less than the failure count.
/// `out_failure`: Receives a caller-owned copy of the failure.
///
/// Returns `HEGEL_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_result_failure(
    ctx: *mut HegelContext,
    r: *const HegelRunResult,
    index: usize,
    out_failure: *mut *mut HegelFailure,
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
    unsafe { *out_failure = ptr::null_mut() };
    let Some(f) = r.failures.get(index) else {
        set_last_error(
            ctx,
            &format!(
                "hegel_run_result_failure: index {index} is out of range \
                 (the result has {} failures)",
                r.failures.len()
            ),
        );
        return HEGEL_E_INVALID_ARG;
    };
    unsafe { *out_failure = into_raw_send_sync(f.clone()) };
    HEGEL_OK
}

/// Parameters:
/// `f`: The failure to free and the strings read off it. Safe to call
///   with NULL.
///
/// Returns `HEGEL_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_failure_free(
    ctx: *mut HegelContext,
    f: *mut HegelFailure,
) -> hegel_result_t {
    clear_last_error(ctx);
    if f.is_null() {
        return HEGEL_OK;
    }
    // SAFETY: `f` is a non-null snapshot from `hegel_run_result_failure` that
    // the caller is freeing exactly once.
    drop(unsafe { Box::from_raw(f) });
    HEGEL_OK
}

/// Parameters:
/// `out_origin`: Receives the origin string the shrinker grouped this
///   bug's probes under. Valid until `hegel_failure_free`.
///
/// Returns `HEGEL_OK`.
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

/// Parameters:
/// `out_blob`: Receives a base64 reproduce blob encoding the minimal
///   counterexample's choice sequence, or NULL if libhegel produced none
///   for this failure. Valid until `hegel_failure_free`.
///
/// Returns `HEGEL_OK`.
///
/// A blob can be replayed later via `hegel_test_case_from_blob` to
/// reproduce the test case exactly. It is only guaranteed to reproduce the
/// failure in the version of Hegel in which it was generated.
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

/// Parameters:
/// `out_version`: Receives libhegel's version string, e.g. `"0.14.12"`.
///   The pointer is static and valid for the program's lifetime.
///
/// Returns `HEGEL_OK`.
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
