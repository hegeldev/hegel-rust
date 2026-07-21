#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::future::Future;
use std::pin::Pin;
use std::ptr;
use std::sync::Arc;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Waker};

use parking_lot::Mutex;

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

use crate::backend::{
    DataSource, DataSourceError, Failure, RunError, TestCaseResult, TestRunResult,
};
use crate::embed::{data_source_for_blob, run_native_async};
use crate::exchange::CaseExchange;
use crate::native::bignum::BigInt;
use crate::settings::{Backend, HealthCheck, Mode, Output, Phase, Settings, Verbosity};

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
#[must_use]
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
    /// required, inverted bounds, a non-UTF-8 string, etc. See
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

    /// An internal invariant failed inside libhegel. Should not happen in
    /// practice; please file a bug. See `hegel_context_last_error()` for the
    /// diagnostic.
    HEGEL_E_INTERNAL = -8,

    /// A single test-case handle was used from two threads at once. Each
    /// handle may be driven by at most one thread at a time; to generate from
    /// several threads, `hegel_test_case_clone` the handle and give each
    /// thread its own clone. (Clones share the underlying test case's
    /// outcome and budgets but generate from independent streams, so they
    /// may be driven concurrently and deterministically.)
    /// Returned by the draw primitives; `hegel_mark_complete` instead waits
    /// for the in-flight operation, because completion always succeeds under
    /// first-caller-wins.
    HEGEL_E_CONCURRENT_USE = -9,
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
///   test case (typically because a `hegel_generate_*` draw returned
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
///
/// A context carries no output destination: that is chosen per run or test
/// case at creation (see `hegel_run_start` / `hegel_test_case_from_blob`).
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
/// NULL `ctx` is a no-op. Skips the allocation when the message is already
/// empty — this runs at the top of every draw in the hot loop.
fn clear_last_error(ctx: *mut HegelContext) {
    if let Some(c) = unsafe { ctx.as_mut() } {
        if !c.last_error.as_bytes().is_empty() {
            c.last_error = CString::default();
        }
    }
}

/// Settings handle for a libhegel run.
///
/// Construct with `hegel_settings_new`, configure via the
/// `hegel_settings_*` family of setters, hand to `hegel_run_start`, then
/// free with `hegel_settings_free`. Settings can be reused across
/// multiple runs; the engine reads them at `hegel_run_start` time.
///
/// A settings handle may be shared across threads once configured — e.g.
/// built once and then handed to `hegel_run_start` from several threads
/// concurrently. The `hegel_settings_set_*` setters mutate the handle, so
/// each setter call requires exclusive access: do not call one concurrently
/// with any other use of the same handle.
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

/// One in-flight test-case handle handed to the caller by
/// `hegel_next_test_case`, `hegel_test_case_from_blob`, or
/// `hegel_test_case_clone`. The caller drives it with the per-test-case
/// primitives (the `hegel_generate_*` draws, `hegel_start_span` /
/// `hegel_stop_span`, `hegel_target`, the collection primitives) and
/// concludes it with `hegel_mark_complete`.
///
/// A single handle must be driven by at most one thread at a time: If
/// multiple threads attempt to use the handle at the same time, operations
/// may raise `HEGEL_E_CONCURRENT_USE` on contention. To use a test case from
/// several threads, clone the handle with `hegel_test_case_clone` and give
/// each thread its own clone.
///
/// Every handle — however it was produced — must be released with
/// `hegel_test_case_free`
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
/// `hegel_run_start` returns one of these. The caller pulls test cases
/// out via `hegel_next_test_case` until it writes NULL through its out
/// parameter, then reads the aggregated outcome via `hegel_run_result`,
/// and finally frees the handle with `hegel_run_free`. There is no
/// background thread: the handle owns the suspended engine as a future,
/// and each `hegel_next_test_case` call resumes it on the calling thread
/// until it offers the next test case (or finishes).
///
/// Unlike test-case handles (which detect and reject concurrent use),
/// a run handle must only be used from one thread at a time: calling
/// `hegel_next_test_case`, `hegel_run_result`, or `hegel_run_free`
/// concurrently on the same run is undefined behavior. In particular,
/// do not free a run from a garbage-collector finalizer thread while
/// another thread may still be using it.
pub struct HegelRun {
    /// The suspended engine. `None` once the run has produced its result —
    /// normally by returning, or abnormally by panicking during a poll (the
    /// panic is caught and converted into an errored result).
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

/// Aggregated outcome of a finished run. `hegel_run_result` writes a
/// caller-owned snapshot of it: read the passed / failed / errored status via
/// `hegel_run_result_status`, the number of distinct failures via
/// `hegel_run_result_failure_count`, each failure via
/// `hegel_run_result_failure(r, i)`, and — for an errored run — the
/// run-level error message via `hegel_run_result_error`. The snapshot is
/// independent of the run (it stays valid after `hegel_run_free`) and must be
/// released with `hegel_run_result_free`; the strings read off it live until
/// then.
#[derive(Clone)]
pub struct HegelRunResult {
    failures: Vec<HegelFailure>,
    /// `Some` iff the run ended in a run-level error instead of a verdict.
    error: Option<CString>,
}

/// One distinct interesting test case surfaced by the run.
/// `hegel_run_result_failure` writes a caller-owned snapshot that owns its
/// strings: reading them via `hegel_failure_origin` /
/// `_reproduction_blob` returns `const char*` pointers that stay valid until
/// the failure is released with `hegel_failure_free`. The snapshot is
/// independent of the result and run it came from.
///
/// A failure carries the origin the engine grouped on and the reproduce blob.
/// The caller replays the blob (via `hegel_test_case_from_blob`) to produce
/// the diagnostic and re-raise the test's own failure.
#[derive(Clone)]
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
/// `*out_settings`. When a CI environment is detected (via `CI`,
/// `GITHUB_ACTIONS`, and similar environment variables) the defaults
/// change: the database is disabled and derandomization is enabled. Use
/// the explicit setters to override either. Must be paired with a
/// `hegel_settings_free` call. Returns `HEGEL_E_INVALID_ARG` if
/// `out_settings` is NULL.
///
/// See `hegel_settings_t` for the threading contract: a configured handle
/// may be shared across threads, but each setter call requires exclusive
/// access.
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
/// one test case. `mode` is a `hegel_mode_t` value; the parameter is typed
/// as `uint32_t` so an out-of-range value from a miscast argument is a
/// reportable `HEGEL_E_INVALID_ARG` instead of undefined behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_settings_set_mode(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    mode: u32,
) -> hegel_result_t {
    clear_last_error(ctx);
    let handle = match unsafe { settings_mut(ctx, s, "hegel_settings_set_mode") } {
        Ok(h) => h,
        Err(rc) => return rc,
    };
    let m = match mode {
        x if x == hegel_mode_t::HEGEL_MODE_TEST_RUN as u32 => Mode::TestRun,
        x if x == hegel_mode_t::HEGEL_MODE_SINGLE_TEST_CASE as u32 => Mode::SingleTestCase,
        _ => {
            set_last_error(
                ctx,
                &format!("hegel_settings_set_mode: unknown mode {mode}"),
            );
            return HEGEL_E_INVALID_ARG;
        }
    };
    handle.inner = handle.inner.clone().mode(m);
    HEGEL_OK
}

/// Select the engine's randomness backend. `backend` is a `hegel_backend_t`
/// value; the parameter is typed as `uint32_t` so an out-of-range value is a
/// reportable `HEGEL_E_INVALID_ARG` instead of undefined behavior.
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

/// Target number of steps to run per stateful test case. The default is
/// 50. Each stateful case runs at least one step and at most `n`; the
/// engine chooses where in that range to stop.
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
    handle.inner = handle.inner.clone().stateful_step_count(n);
    HEGEL_OK
}

/// Set the engine's output verbosity. `v` is a `hegel_verbosity_t` value;
/// the parameter is typed as `uint32_t` so an out-of-range value is a
/// reportable `HEGEL_E_INVALID_ARG` instead of undefined behavior.
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
/// (e.g. you intentionally have a high rejection rate). Each call replaces
/// the full set of suppressed checks, so passing 0 clears any previous
/// suppression.
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

static ENGINE_PANIC_HOOK: Once = Once::new();

thread_local! {
    /// Set for the duration of an engine poll (see [`EnginePollGuard`]) so
    /// the panic hook can recognise engine panics. Keying on this rather
    /// than anything about the thread means the embedding application's own
    /// panics on the same thread are reported normally.
    static IN_ENGINE_POLL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// RAII guard marking the current thread as inside an engine poll for the
/// panic hook, cleared on drop — including the unwind of a caught engine
/// panic.
struct EnginePollGuard;

impl EnginePollGuard {
    fn enter() -> Self {
        IN_ENGINE_POLL.with(|f| f.set(true));
        EnginePollGuard
    }
}

impl Drop for EnginePollGuard {
    fn drop(&mut self) {
        IN_ENGINE_POLL.with(|f| f.set(false));
    }
}

/// Install (once) a process-global panic hook that swallows the default
/// `thread '…' panicked at <file>:<line>:<col>` stderr line for panics
/// raised while the engine is being polled.
///
/// Every engine panic (an internal invariant, an invalid-argument usage
/// error) is raised inside a poll, is already caught by the poll's
/// `catch_unwind`, and is surfaced as a run-level error through
/// `hegel_run_result_error`. Letting the default hook *also* dump a
/// Rust-internal source location to the embedding process's stderr is pure
/// noise — a C consumer has no use for `src/native/test_runner.rs:329:21`,
/// and it leaks implementation detail. Panics outside an engine poll
/// (notably from the caller's own code) fall through to the previous hook
/// unchanged.
fn install_engine_panic_hook() {
    ENGINE_PANIC_HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if IN_ENGINE_POLL.try_with(|f| f.get()).unwrap_or(false) {
                return; // nocov
            }
            prev(info);
        }));
    });
}

/// Start a property-test run with the given settings, writing a handle the
/// caller pulls test cases out of via `hegel_next_test_case` into `*out_run`.
///
/// This only builds the run: no test case is generated until the first
/// `hegel_next_test_case` call, and all engine work happens on the thread
/// making those calls. The caller does not need to hold the settings handle
/// alive — `hegel_run_start` snapshots the settings it needs.
///
/// `callback` sets where the engine's output for this run goes: each line is
/// delivered to it (with `user_data` passed through verbatim) instead of
/// stderr, once per line, NUL-terminated UTF-8 of `len` bytes without a
/// trailing newline, in a buffer owned by libhegel and valid only for the
/// duration of the call. A NULL `callback` leaves the run's output on stderr
/// (`user_data` is ignored). The engine emits while it runs inside
/// `hegel_next_test_case`, so the callback is invoked on whichever thread
/// makes that call, and it — along with whatever `user_data` points to —
/// must stay valid until the run has been freed with `hegel_run_free`.
/// Because it runs inside `hegel_next_test_case`, while the run handle is in
/// use, the callback must not call back into libhegel on the same run (e.g.
/// `hegel_next_test_case` or `hegel_run_free`). This sets only the
/// *destination*; how much output the engine emits is controlled by
/// `hegel_settings_set_verbosity`.
///
/// Returns `HEGEL_E_INVALID_ARG` for a NULL `out_run` or
/// `HEGEL_E_INVALID_HANDLE` for a NULL `settings`. The handle written to
/// `*out_run` must be freed with `hegel_run_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_run_start(
    ctx: *mut HegelContext,
    settings: *const HegelSettings,
    callback: hegel_output_callback_t,
    user_data: *mut c_void,
    out_run: *mut *mut HegelRun,
) -> hegel_result_t {
    clear_last_error(ctx);
    install_engine_panic_hook();
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

/// Run the engine on the calling thread until it produces the next test case,
/// writing a handle for it into `*out_test_case`.
///
/// The handle is owned by the caller and must be released with
/// `hegel_test_case_free` (the run keeps its own internal reference, so freeing
/// the handle never disturbs the run). When the run is finished this writes
/// NULL into `*out_test_case` and returns
/// `HEGEL_OK`; call `hegel_run_result` to read the outcome. A non-`HEGEL_OK`
/// code means something went wrong (caller misuse, engine crash) rather than
/// normal completion: `HEGEL_E_NOT_COMPLETE` if the previous test case was not
/// marked complete (call `hegel_mark_complete` first), `HEGEL_E_INVALID_HANDLE`
/// for a NULL `run`, or `HEGEL_E_INVALID_ARG` for a NULL `out_test_case`.
///
/// All engine work between test cases — generation, mutation, shrinking —
/// happens inside this call, so a call may take a while when the engine has
/// exploring to do.
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
        Ok(Poll::Pending) => {
            let family = new_family(run.exchange.take());
            let case = handle_from_family(Arc::clone(&family));
            run.current_family = Some(family);
            unsafe { *out_test_case = case };
            HEGEL_OK
        }
        Ok(Poll::Ready(r)) => {
            run.result = Some(match r {
                Ok(r) => HegelRunResult::from(r),
                Err(run_error) => HegelRunResult::from_error(&run_error.to_string()),
            });
            run.engine = None;
            HEGEL_OK
        }
        Err(payload) => {
            run.result = Some(HegelRunResult::from_error(&format!(
                "Engine panic: {}",
                crate::panic::panic_message(&payload)
            )));
            run.engine = None;
            HEGEL_OK
        }
    }
}

/// Resume the engine until it offers the next test case (`Pending`) or the
/// run finishes (`Ready`), catching engine panics so they surface as a
/// run-level error instead of unwinding into the C caller. The engine only
/// suspends at its case exchange and is only resumed here, so a no-op waker
/// suffices — no executor is involved.
fn poll_engine(
    engine: &mut EngineFuture,
) -> Result<Poll<Result<TestRunResult, RunError>>, Box<dyn std::any::Any + Send>> {
    let _guard = EnginePollGuard::enter();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
    }))
}

/// Write a caller-owned snapshot of the aggregated result of a finished run
/// into `*out_result`. Returns `HEGEL_E_NOT_COMPLETE` with
/// `hegel_context_last_error` set if the run hasn't finished yet
/// (`hegel_next_test_case` has not yet reported completion on this run),
/// `HEGEL_E_INVALID_HANDLE` for a NULL `run`, or `HEGEL_E_INVALID_ARG` for a
/// NULL `out_result`.
///
/// The snapshot is independent of the run: it stays valid after
/// `hegel_run_free` and must be released with `hegel_run_result_free`. Each
/// call writes a fresh snapshot, each freed separately.
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

/// Release a run-result snapshot from `hegel_run_result`, along with the
/// strings read off it. Safe to call with NULL (a no-op that returns
/// `HEGEL_OK`). Must be called exactly once per snapshot; freeing the same
/// snapshot twice is undefined behaviour.
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

/// Free a run handle. Safe to call with NULL (a no-op that returns
/// `HEGEL_OK`). Result and failure snapshots from `hegel_run_result` /
/// `hegel_run_result_failure` are independent of the run and stay valid;
/// they are released with their own frees.
///
/// If the caller exited its test loop early (e.g. with a still-active
/// test case), any in-flight test case is marked complete and the rest of
/// the exploration is simply dropped — the engine was suspended waiting for
/// the next `hegel_next_test_case` call, so there is nothing to wind down.
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

/// Build a standalone test case that replays the example encoded in a
/// base64 failure blob (obtained from `hegel_failure_reproduction_blob` on a
/// prior run).
///
/// There is no run handle and no engine run: the caller drives the
/// returned test case with the usual per-test-case primitives
/// (the `hegel_generate_*` draws, spans, …), concludes it with `hegel_mark_complete`,
/// and decides for itself whether the blob reproduced the failure (the
/// property failed again) or is stale (it passed). Replay several blobs by
/// calling this once per blob. A blob whose choices no longer match the
/// caller's generators surfaces as `HEGEL_E_STOP_TEST` from the draw that
/// overruns. Replaying a blob is how a caller performs the *final replay* of
/// a counterexample.
///
/// `callback` sets where the engine's output for this replay goes — at debug
/// verbosity the blob is decoded with a trace line, emitted synchronously
/// during this call. Each line is delivered to `callback` (with `user_data`
/// passed through verbatim) instead of stderr, NUL-terminated UTF-8 of `len`
/// bytes without a trailing newline, in a buffer valid only for the duration
/// of the call. A NULL `callback` leaves the replay's output on stderr
/// (`user_data` is ignored). The callback is only ever invoked on this
/// thread and need not outlive this call.
///
/// Returns `HEGEL_E_INVALID_HANDLE` for a NULL `s`, or `HEGEL_E_INVALID_ARG`
/// for a NULL `out_test_case`, a NULL `blob`, or a `blob` that is not a valid
/// failure blob (corrupt, non-UTF-8, or from an incompatible Hegel version),
/// with a diagnostic in `hegel_context_last_error`. The handle written to
/// `*out_test_case` is owned by the **caller** and must be released with
/// `hegel_test_case_free`, like every test-case handle.
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

/// Release a test-case handle, whatever its origin — a handle from
/// `hegel_test_case_from_blob`, a clone from `hegel_test_case_clone`, or a
/// run-owned handle from `hegel_next_test_case`. Safe to call with NULL (a
/// no-op that returns `HEGEL_OK`), and safe whether or not the test case was
/// marked complete.
///
/// Each handle holds one reference to the shared test case. Freeing it drops
/// that reference; the underlying data source is released once the last
/// reference is gone (every handle freed, and — for a run-owned family — the
/// run has released its own reference). Each handle must be freed exactly once;
/// freeing the same handle twice is undefined behaviour.
///
/// Freeing is not completing: a run-owned test case still needs
/// `hegel_mark_complete` from some handle in its family before the run can
/// advance. Freeing the last handle of an uncompleted run-owned family leaves
/// `hegel_next_test_case` returning `HEGEL_E_NOT_COMPLETE` with no way to
/// complete the case, and the run can then only be torn down with
/// `hegel_run_free` — so conclude every case before dropping your last handle
/// to it.
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

/// Clone a test-case handle, writing a new handle onto an *independent
/// stream* of the same test case into `*out_test_case`.
///
/// The clone shares the test case's outcome — `hegel_mark_complete` on any
/// handle in the family marks them all complete, and budgets are shared —
/// but generates from its own independent choice sequence. The clone and
/// the handle it came from can therefore be driven concurrently from
/// different threads without perturbing each other, and the values each
/// produces are deterministic under replay and shrink correctly. (Whereas
/// using a *single* handle from two threads returns
/// `HEGEL_E_CONCURRENT_USE`.) Collections, variable pools, and state
/// machines remain shared across the family — ids from one handle work on
/// any other — but *concurrent* use of one such object from two streams
/// makes the affected values scheduling-dependent.
///
/// Cloning is a stream operation: it occupies one choice position on the
/// source handle's stream, takes the source handle's lock like a draw
/// (`HEGEL_E_CONCURRENT_USE` if another thread is mid-operation on it), and
/// fails with `HEGEL_E_ALREADY_COMPLETE` once the family has completed.
/// Cloning a clone creates a further independent stream.
///
/// The new handle holds its own reference to the shared test case and must be
/// released with `hegel_test_case_free`, like any other handle. The underlying
/// test case stays alive until every handle (this clone, the handle it was
/// cloned from, and any others) has been freed.
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
) -> Result<(&'a HegelTestCase, parking_lot::MutexGuard<'a, LocalState>), hegel_result_t> {
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
) -> Result<(&'a HegelTestCase, parking_lot::MutexGuard<'a, LocalState>), hegel_result_t> {
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
    }
}

/// Reconstruct and drop an engine-allocated buffer handed out by a
/// `hegel_generate_*` draw. `data` must come from `Box::into_raw` on a boxed
/// `[u8]` of length `len` and must not be freed again.
unsafe fn free_engine_buffer(data: *mut u8, len: usize) {
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len)) });
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_start_span", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.stream.start_span(label) {
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_stop_span", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    match tc.stream.stop_span(discard) {
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_new_collection", tc) } {
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_collection_more", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_more.is_null() {
        set_last_error(ctx, "hegel_collection_more: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.stream.collection_more(collection_id) {
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
/// rejection reason (NULL is allowed); it is validated but currently
/// unused, reserved for future rejection diagnostics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_collection_reject(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    collection_id: i64,
    why: *const c_char,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_collection_reject", tc) } {
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
    match tc.stream.collection_reject(collection_id, why_str) {
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_new_pool", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_pool_id.is_null() {
        set_last_error(ctx, "hegel_new_pool: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.stream.new_pool() {
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_pool_add", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error(ctx, "hegel_pool_add: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.stream.pool_add(pool_id) {
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
/// returns `HEGEL_OK`. Returns `HEGEL_E_ASSUME` if the pool currently
/// has no active variables — the caller should treat that like any other
/// failed assumption: it may recover and continue the test case (as
/// stateful testing does when a rule's assumption fails, by skipping the
/// action), or give up on the case and mark it INVALID.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hegel_pool_generate(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    pool_id: i64,
    consume: bool,
    out_variable_id: *mut i64,
) -> hegel_result_t {
    clear_last_error(ctx);
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_pool_generate", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_variable_id.is_null() {
        set_last_error(ctx, "hegel_pool_generate: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.stream.pool_generate(pool_id, consume) {
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
/// applies it, until that call signals that no more steps should
/// follow.
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_new_state_machine", tc) } {
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
    match tc.stream.new_state_machine(rules, invariants) {
        Ok(id) => {
            unsafe { *out_state_machine_id = id };
            HEGEL_OK
        }
        Err(e) => translate_ds_error(ctx, e),
    }
}

/// Value written to `*out_rule_index` by `hegel_state_machine_next_rule`
/// when the engine's step budget for the test case is exhausted: stop
/// running rules.
pub const HEGEL_STATE_MACHINE_DONE: i64 = -1;

/// Draw the index of the next rule to run, in `[0, num_rules)`, letting
/// the engine choose (and shrink) the rule sequence. Swarm testing is
/// applied per test case: a random subset of rules is enabled on the
/// first call and selection is restricted to that subset for the rest
/// of the test case, with restrictions that shrink away in minimal
/// counterexamples.
///
/// `state_machine_id` must be an id returned by
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
    let (tc, _guard) = match unsafe { tc_guard(ctx, "hegel_state_machine_next_rule", tc) } {
        Ok(t) => t,
        Err(rc) => return rc,
    };
    if out_rule_index.is_null() {
        set_last_error(ctx, "hegel_state_machine_next_rule: out parameter is null");
        return HEGEL_E_INVALID_ARG;
    }
    match tc.stream.state_machine_next_rule(state_machine_id) {
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

/// Draw an integer in `[min_value, max_value]` (both inclusive, both
/// required). The engine biases toward boundary values and shrinks toward
/// zero. For bounds outside the `int64_t` range use
/// `hegel_generate_integer_big`.
///
/// On success writes the drawn value into `*out_value` and returns
/// `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
/// is exhausted for this test case (the caller should abort the body and
/// call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`). Returns
/// `HEGEL_E_INVALID_ARG` for a NULL `out_value` or `min_value > max_value`;
/// the diagnostic is in `hegel_context_last_error`.
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
                tc.stream
                    .generate_integer(&BigInt::from(min_value), &BigInt::from(max_value))
            },
            |v| *out_value = i64::try_from(v).expect("value validated to fit the i64 bounds"),
        )
    }
}

/// Draw an arbitrary-precision integer in `[min_value, max_value]`.
///
/// Bounds and result are two's-complement **little-endian** signed byte
/// buffers (the natural encoding of Go's `math/big` `FillBytes` reversed, or
/// Rust's `i128::to_le_bytes` for fixed-width values). Both bounds are
/// required and must be non-empty.
///
/// On success writes the drawn value's two's-complement little-endian bytes
/// into `out_value` (capacity `out_value_cap`), its minimal length into
/// `*out_value_len`, sign-fills the rest of the buffer up to
/// `out_value_cap` (so reading the whole buffer as a fixed-width
/// two's-complement integer also yields the drawn value, with no
/// sign-extension needed on the caller's side), and returns `HEGEL_OK`. A
/// value in range never needs more bytes than the longer of the two bound
/// encodings, so passing
/// `out_value_cap >= max(min_value_len, max_value_len)` always succeeds.
/// Returns `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted
/// for this test case. Returns `HEGEL_E_INVALID_ARG` for NULL or empty
/// bounds, NULL out parameters, `min_value > max_value`, or an `out_value`
/// buffer too small for the drawn value; the diagnostic is in
/// `hegel_context_last_error`.
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
    let min_bytes = unsafe { std::slice::from_raw_parts(min_value, min_value_len) };
    let max_bytes = unsafe { std::slice::from_raw_parts(max_value, max_value_len) };
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
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_value, bytes.len());
                std::ptr::write_bytes(
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

/// Draw a float of the given `width` (32 or 64) in
/// `[min_value, max_value]`.
///
/// Pass `-INFINITY` / `INFINITY` for unbounded ends. NaN is drawn only when
/// `allow_nan` is set; infinities only when `allow_infinity` is set and the
/// relevant endpoint is unbounded. `exclude_min` / `exclude_max` make the
/// corresponding bound exclusive by stepping it to the next representable
/// value at the requested width. Nonzero magnitudes below
/// `smallest_nonzero_magnitude` are never drawn — it must be positive and
/// finite; pass `5e-324` (width 64) or the smallest `float` subnormal
/// (width 32) for no restriction. Width-32 bounds must be exactly
/// representable as `float`, and finite width-32 results are exactly
/// representable as `float`.
///
/// On success writes the drawn value into `*out_value` and returns
/// `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
/// is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a NULL
/// `out_value`, an unsupported width, NaN bounds, width-32 bounds that are
/// not exactly representable as `float`, an invalid
/// `smallest_nonzero_magnitude`, or an empty range; the diagnostic is in
/// `hegel_context_last_error`.
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

/// Draw a byte string with length in `[min_size, max_size]` (both
/// inclusive).
///
/// On success fills `*out_result` with an engine-allocated buffer the caller
/// owns (release with `hegel_generate_bytes_result_free`) and returns
/// `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
/// is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a NULL
/// `out_result` or `min_size > max_size`; the diagnostic is in
/// `hegel_context_last_error`.
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

/// Release a buffer returned by `hegel_generate_bytes` and reset the struct
/// to `{NULL, 0}`. Safe to call with a NULL `result` or an already-freed
/// (zeroed) struct — both are no-ops that return `HEGEL_OK`.
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

/// Opaque specification of a string draw — the alphabet-and-shape half of
/// `hegel_generate_string`.
///
/// Build one with a `hegel_string_generator_*` constructor (text, regex,
/// email, url, domain); every parameter is validated at construction so a
/// bad alphabet or pattern is reported immediately rather than mid-draw.
/// A generator is immutable after construction and may be shared freely
/// across test cases and threads. Free it with
/// `hegel_string_generator_free` once no draws will use it again.
pub struct HegelStringGenerator {
    spec: crate::native::draws::StringSpec,
}

/// Translate a constructor-time engine error onto `ctx`. Constructors
/// perform no draws, so any error they report is by definition an invalid
/// argument.
fn translate_construct_error(
    ctx: *mut HegelContext,
    e: crate::native::core::EngineError,
) -> hegel_result_t {
    set_last_error(ctx, &e.to_string());
    HEGEL_E_INVALID_ARG
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
    let bytes = unsafe { std::slice::from_raw_parts(p, len) };
    match std::str::from_utf8(bytes) {
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

/// Build a **text** string generator: strings with length in
/// `[min_size, max_size]` whose characters are drawn from the described
/// alphabet.
///
/// The alphabet starts from `codec`'s range — `"ascii"`, `"latin-1"` /
/// `"iso-8859-1"`, or `"utf-8"` / NULL for all of Unicode — intersected
/// with `[min_codepoint, max_codepoint]` (pass `0` and `UINT32_MAX` for no
/// constraint; surrogates are always removed). `categories` restricts to
/// the union of the named Unicode general categories (NULL for no
/// restriction; a non-NULL empty list means an empty alphabet), and
/// `exclude_categories` removes categories. `include_characters` /
/// `exclude_characters` are UTF-8 buffers (pointer + byte length; NULL for
/// none) of individual characters unioned in / removed last. They are
/// length-delimited rather than NUL-terminated because U+0000 is a valid
/// character to include or exclude.
///
/// On success writes a caller-owned handle into `*out_generator` (release
/// with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
/// `HEGEL_E_INVALID_ARG` — with a diagnostic in `hegel_context_last_error`
/// — for a NULL `out_generator`, `min_size > max_size`, an unknown codec or
/// category, non-UTF-8 string arguments, include/exclude conflicts, or
/// constraints that leave no characters while `max_size > 0`.
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

/// Build a **regex** string generator: strings matching `pattern`
/// (Python-`re` syntax). When `fullmatch` is true the whole string matches
/// the pattern; otherwise the match may be padded on either side.
/// `alphabet` — optional (NULL for none) — must be a **text** generator; its
/// character set constrains the padding and wildcard characters.
///
/// On success writes a caller-owned handle into `*out_generator` (release
/// with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
/// `HEGEL_E_INVALID_ARG` — with a diagnostic in `hegel_context_last_error`
/// — for a NULL `out_generator`, a NULL / non-UTF-8 / invalid `pattern`, or
/// an `alphabet` that is not a text generator.
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

/// Build an **email** string generator producing RFC 5321/5322 addresses
/// like `alice@example.com`.
///
/// On success writes a caller-owned handle into `*out_generator` (release
/// with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
/// `HEGEL_E_INVALID_ARG` for a NULL `out_generator`.
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

/// Build a **URL** string generator producing RFC 3986 `http`/`https` URLs.
///
/// On success writes a caller-owned handle into `*out_generator` (release
/// with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
/// `HEGEL_E_INVALID_ARG` for a NULL `out_generator`.
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

/// Build a **domain-name** string generator producing RFC 1035
/// fully-qualified domain names of total length at most `max_length`
/// (4..=255; RFC 1035 §2.3.4 allows 255).
///
/// On success writes a caller-owned handle into `*out_generator` (release
/// with `hegel_string_generator_free`) and returns `HEGEL_OK`. Returns
/// `HEGEL_E_INVALID_ARG` — with a diagnostic in `hegel_context_last_error`
/// — for a NULL `out_generator` or a `max_length` that leaves no eligible
/// top-level domains.
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

/// Release a string generator built by a `hegel_string_generator_*`
/// constructor. Safe to call with NULL (a no-op that returns `HEGEL_OK`).
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

/// Draw a string described by `generator` (built with a
/// `hegel_string_generator_*` constructor).
///
/// On success fills `*out_result` with an engine-allocated UTF-8 buffer the
/// caller owns (release with `hegel_generate_string_result_free`) and
/// returns `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice
/// budget is exhausted for this test case (the caller should abort the body
/// and call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`), and
/// `HEGEL_E_ASSUME` when the draw rejected itself (e.g. an email exceeding
/// the RFC length cap; discard the test case as invalid). Returns
/// `HEGEL_E_INVALID_HANDLE` for a NULL `tc` or `generator`, and
/// `HEGEL_E_INVALID_ARG` for a NULL `out_result`.
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

/// Release a buffer returned by `hegel_generate_string` and reset the
/// struct to `{NULL, 0}`. Safe to call with a NULL `result` or an
/// already-freed (zeroed) struct — both are no-ops that return `HEGEL_OK`.
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

/// Draw a Gregorian calendar date in `[min_value, max_value]` (both
/// inclusive), shrinking toward 2000-01-01, or the nearest bound when that
/// is out of range. Bounds are proleptic Gregorian dates with `year` in
/// `[-999999, 999999]`; pass `{1, 1, 1}` and `{9999, 12, 31}` for the
/// conventional full range.
///
/// On success writes the drawn date into `*out_value` and returns
/// `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
/// is exhausted for this test case (the caller should abort the body and
/// call `hegel_mark_complete` with `HEGEL_STATUS_OVERRUN`). Returns
/// `HEGEL_E_INVALID_ARG` for a NULL `out_value`, an invalid calendar date
/// in either bound, or `min_value > max_value`.
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

/// Draw a time of day in `[min_value, max_value]` (both inclusive),
/// shrinking toward `min_value` (the representable time closest to
/// midnight). Pass all-zeros and `{23, 59, 59, 999999}` for the full day.
///
/// On success writes the drawn time into `*out_value` and returns
/// `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
/// is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a
/// NULL `out_value`, an out-of-range field in either bound, or
/// `min_value > max_value`.
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

/// Draw a naive datetime (no timezone) in `[min_value, max_value]` (both
/// inclusive), shrinking toward 2000-01-01T00:00:00 clamped into range: a
/// bounded date draw, then a time draw whose bounds tighten to the endpoint
/// times when the drawn date lands on a boundary date.
///
/// On success writes the drawn datetime into `*out_value` and returns
/// `HEGEL_OK`. Returns `HEGEL_E_STOP_TEST` when the engine's choice budget
/// is exhausted for this test case. Returns `HEGEL_E_INVALID_ARG` for a
/// NULL `out_value`, an invalid date or time in either bound, or
/// `min_value > max_value`.
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

/// Draw a UUID as 16 big-endian bytes written to `out_bytes` (which must
/// have room for 16 bytes).
///
/// When `has_version` is set, the RFC 4122 version nibble is forced to
/// `version` (a single hex nibble, 0..=15 — conventionally 1..=5) and the
/// variant nibble to the RFC 4122 variant. Without a version the 128 bits
/// are uniform, except that the nil UUID is never produced.
///
/// On success writes 16 bytes and returns `HEGEL_OK`. Returns
/// `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted for
/// this test case. Returns `HEGEL_E_INVALID_ARG` for a NULL `out_bytes` or
/// a `version > 15`.
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
            |bytes| std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_bytes, 16),
        )
    }
}

/// Draw an IPv4 address. Half the draws are uniform over the whole address
/// space and half are biased into the IANA special-purpose ranges
/// (loopback, private, documentation, …).
///
/// On success writes the address's 4 network-order bytes into `out_bytes`
/// (which must have room for 4 bytes) and returns `HEGEL_OK`. Returns
/// `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted for
/// this test case, and `HEGEL_E_INVALID_ARG` for a NULL `out_bytes`.
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
                std::ptr::copy_nonoverlapping(octets.as_ptr(), out_bytes, 4);
            },
        )
    }
}

/// Draw an IPv6 address, with the same special-range biasing as
/// `hegel_generate_ipv4`.
///
/// On success writes the address's 16 network-order bytes into `out_bytes`
/// (which must have room for 16 bytes) and returns `HEGEL_OK`. Returns
/// `HEGEL_E_STOP_TEST` when the engine's choice budget is exhausted for
/// this test case, and `HEGEL_E_INVALID_ARG` for a NULL `out_bytes`.
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
                std::ptr::copy_nonoverlapping(octets.as_ptr(), out_bytes, 16);
            },
        )
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
///
/// Completing a test case is **first-caller-wins and family-wide**: the first
/// `hegel_mark_complete` anywhere in the family (any clone or the root) records
/// the outcome and unblocks the run. A later call on a *different* handle in the
/// family is then a safe no-op that returns `HEGEL_OK`, so two clones racing to
/// complete the same test case do not error — whichever wins sets the result.
/// Calling `hegel_mark_complete` on the *same* handle twice is a usage error and
/// returns `HEGEL_E_ALREADY_COMPLETE`. Because completion always succeeds under
/// first-caller-wins, `hegel_mark_complete` never returns
/// `HEGEL_E_CONCURRENT_USE`: if another thread is mid-operation on this handle
/// it waits for that operation to finish and then completes. A NULL `tc`
/// returns `HEGEL_E_INVALID_HANDLE`; a non-UTF-8 `origin` returns
/// `HEGEL_E_INVALID_ARG`.
///
/// `status` is a `hegel_status_t` value; the parameter is typed as
/// `uint32_t` so an out-of-range value is a reportable
/// `HEGEL_E_INVALID_ARG` instead of undefined behavior.
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
/// The written pointer is owned by the result snapshot and valid until
/// `hegel_run_result_free`. Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r` or
/// `HEGEL_E_INVALID_ARG` for a NULL `out_error`.
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

/// Write a caller-owned snapshot of the `index`-th failure (0-based) into
/// `*out_failure`. `index` must be less than
/// `hegel_run_result_failure_count(r)`. The snapshot is independent of the
/// result and run it came from and must be released with
/// `hegel_failure_free`; each call writes a fresh snapshot, each freed
/// separately. Returns `HEGEL_E_INVALID_HANDLE` for a NULL `r`, or
/// `HEGEL_E_INVALID_ARG` for a NULL `out_failure` or an out-of-range `index`
/// (with a diagnostic in `hegel_context_last_error`).
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

/// Release a failure snapshot from `hegel_run_result_failure`, along with the
/// strings read off it. Safe to call with NULL (a no-op that returns
/// `HEGEL_OK`). Must be called exactly once per snapshot; freeing the same
/// snapshot twice is undefined behaviour.
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
/// produced no blob for this failure. The written pointer is owned by the
/// failure snapshot and stays valid until `hegel_failure_free`. Returns
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
