//! The libhegel C-ABI boundary.
//!
//! hegeltest drives the engine the same way every other language binding
//! does: through the `hegel_*` C functions exported by the `hegel-c` crate
//! (lib name `hegel_c`), passing CBOR bytes and opaque handles and reading
//! back `hegel_result_t` codes. This module is the single place that touches
//! those raw functions; the rest of the frontend works against the safe
//! wrappers here.
//!
//! The wrappers deliberately do *not* know about hegeltest's control-flow
//! unwinds: the per-test-case methods return `Result<_, hegel_result_t>` and
//! leave it to [`crate::test_case`] to translate a non-`HEGEL_OK` code into the
//! right [`crate::control`] payload (a `StopTest` / `AssumeFailed` /
//! invalid-argument unwind). Keeping that split means the unsafe boundary stays
//! small and the control-flow policy stays with the test lifecycle.

use crate::runner::{Backend, Database, HealthCheck, Mode, Phase, Settings, Verbosity};
use hegel_c::hegel_result_t;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

/// Owns a `*mut HegelContext` — libhegel's explicit per-call error channel —
/// and frees it on drop.
struct Context {
    raw: *mut hegel_c::HegelContext,
}

impl Context {
    fn new() -> Self {
        // SAFETY: hegel_context_new never returns null.
        Context {
            raw: hegel_c::hegel_context_new(),
        }
    }

    fn as_ptr(&self) -> *mut hegel_c::HegelContext {
        self.raw
    }

    /// Copy out the most recent error message recorded on this context.
    /// libhegel's buffer is borrowed and invalidated by the next call taking
    /// this context, so we copy immediately.
    fn last_error(&self) -> String {
        // SAFETY: self.raw is a live, non-null context handle.
        let p = unsafe { hegel_c::hegel_context_last_error(self.raw) };
        if p.is_null() {
            return String::new(); // nocov
        }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: `raw` came from hegel_context_new and is freed exactly once.
        require_ok(unsafe { hegel_c::hegel_context_free(self.raw) });
    }
}

thread_local! {
    /// This thread's libhegel error context.
    ///
    /// libhegel reports a failed call's diagnostic on an explicit context
    /// handle rather than its own thread-local state — deliberately, so that
    /// callers running on green threads / fibers that migrate between OS
    /// threads mid-call (e.g. Go's goroutines) are not pinned to thread-local
    /// error storage. The hegel-rust frontend has no such constraint: it
    /// drives the engine from ordinary OS threads, and a failed call and the
    /// [`last_error_string`] that reads its message always run on the same
    /// thread. So it is sound — and simplest — to keep one context per thread
    /// here and pass it to every fallible call. Each thread gets its own, so
    /// no two threads ever share one (and it is freed when the thread exits).
    static CONTEXT: Context = Context::new();
}

/// Run `f` with this thread's libhegel error-context pointer.
fn with_context<R>(f: impl FnOnce(*mut hegel_c::HegelContext) -> R) -> R {
    CONTEXT.with(|c| f(c.as_ptr()))
}

/// The most recent error message libhegel recorded on this thread's context,
/// or an empty string if the last call on it succeeded. Read synchronously on
/// the same thread that made the failing call, before any later call can
/// overwrite it.
pub(crate) fn last_error_string() -> String {
    CONTEXT.with(|c| c.last_error())
}

/// Build a NUL-terminated C string, replacing any interior NUL (which a
/// `CString` cannot represent) with the Unicode replacement character so the
/// value still round-trips to a diagnostic rather than being dropped.
fn cstring_lossy(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|_| CString::new(s.replace('\0', "\u{FFFD}")).unwrap())
}

/// Owns a `*mut HegelSettings` and frees it on drop. Built from a frontend
/// [`Settings`] plus its database key by [`SettingsHandle::build`].
pub(crate) struct SettingsHandle {
    raw: *mut hegel_c::HegelSettings,
}

impl SettingsHandle {
    /// Materialize a libhegel settings handle from the frontend settings,
    /// translating every field through the corresponding `hegel_settings_*`
    /// setter. `print_blob` has no setter — the blob is always returned by the
    /// engine and printing is a frontend decision — so it is intentionally not
    /// forwarded here.
    pub(crate) fn build(settings: &Settings, database_key: Option<&str>) -> Self {
        with_context(|ctx| {
            let mut raw: *mut hegel_c::HegelSettings = ptr::null_mut();
            // SAFETY: ctx is this thread's live context; &mut raw is a valid
            unsafe {
                require_ok(hegel_c::hegel_settings_new(ctx, &mut raw));
                require_ok(hegel_c::hegel_settings_set_mode(
                    ctx,
                    raw,
                    map_mode(settings.mode),
                ));
                require_ok(hegel_c::hegel_settings_set_test_cases(
                    ctx,
                    raw,
                    settings.test_cases,
                ));
                require_ok(hegel_c::hegel_settings_set_verbosity(
                    ctx,
                    raw,
                    map_verbosity(settings.verbosity),
                ));
                require_ok(match settings.seed {
                    Some(seed) => hegel_c::hegel_settings_set_seed(ctx, raw, seed, true),
                    None => hegel_c::hegel_settings_set_seed(ctx, raw, 0, false),
                });
                require_ok(hegel_c::hegel_settings_set_derandomize(
                    ctx,
                    raw,
                    settings.derandomize,
                ));
                require_ok(hegel_c::hegel_settings_set_report_multiple_failures(
                    ctx,
                    raw,
                    settings.report_multiple_failures,
                ));
                match &settings.database {
                    Database::Disabled => {
                        let empty = CString::new("").unwrap();
                        require_ok(hegel_c::hegel_settings_set_database(
                            ctx,
                            raw,
                            empty.as_ptr(),
                        ));
                    }
                    Database::Path(path) => {
                        let c = cstring_lossy(path);
                        require_ok(hegel_c::hegel_settings_set_database(ctx, raw, c.as_ptr()));
                    }
                    Database::Unset => {}
                }
                if let Some(key) = database_key {
                    let c = cstring_lossy(key);
                    require_ok(hegel_c::hegel_settings_set_database_key(
                        ctx,
                        raw,
                        c.as_ptr(),
                    ));
                }
                require_ok(hegel_c::hegel_settings_set_phases(
                    ctx,
                    raw,
                    phases_bitmask(&settings.phases),
                ));
                require_ok(hegel_c::hegel_settings_set_suppress_health_check(
                    ctx,
                    raw,
                    health_check_bitmask(&settings.suppress_health_check),
                ));
                require_ok(hegel_c::hegel_settings_set_backend(
                    ctx,
                    raw,
                    map_backend(settings.backend),
                ));
            }
            SettingsHandle { raw }
        })
    }

    pub(crate) fn as_ptr(&self) -> *const hegel_c::HegelSettings {
        self.raw
    }
}

impl Drop for SettingsHandle {
    fn drop(&mut self) {
        // SAFETY: `raw` came from hegel_settings_new and is freed exactly once.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_settings_free(ctx, self.raw)
        }));
    }
}

/// Owns a `*mut HegelRun` and frees it on drop (which aborts and joins the
/// engine worker if the run was not drained to completion).
pub(crate) struct RunHandle {
    raw: *mut hegel_c::HegelRun,
}

impl RunHandle {
    /// Start a run. Returns `Err` with libhegel's diagnostic if the engine
    /// could not be started.
    pub(crate) fn start(settings: &SettingsHandle) -> Result<Self, String> {
        let mut raw: *mut hegel_c::HegelRun = ptr::null_mut();
        // SAFETY: settings.as_ptr() is a live, non-null handle; &mut raw is a
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_run_start(ctx, settings.as_ptr(), &mut raw)
        });
        if rc != hegel_result_t::HEGEL_OK {
            return Err(last_error_string()); // nocov
        }
        Ok(RunHandle { raw })
    }

    /// Pull the next test case the engine wants to run, or `None` when the run
    /// is finished. The returned handle holds its own reference to the test
    /// case (the run keeps a separate reference internally), so the frontend
    /// owns it and frees it on drop.
    pub(crate) fn next_test_case(&self) -> Option<CTestCase> {
        let mut raw: *mut hegel_c::HegelTestCase = ptr::null_mut();
        // SAFETY: self.raw is a live run handle; libhegel blocks until the next
        let rc =
            with_context(|ctx| unsafe { hegel_c::hegel_next_test_case(ctx, self.raw, &mut raw) });
        if rc != hegel_result_t::HEGEL_OK || raw.is_null() {
            None
        } else {
            Some(CTestCase { raw })
        }
    }

    /// Read the aggregate result. Borrowed from the run; valid until the run
    /// is dropped.
    pub(crate) fn result(&self) -> RunResult<'_> {
        let mut raw: *const hegel_c::HegelRunResult = ptr::null();
        // SAFETY: called after the pull loop drained; libhegel writes a borrowed
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result(ctx, self.raw, &mut raw)
        }));
        RunResult {
            raw,
            _run: std::marker::PhantomData,
        }
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        // SAFETY: `raw` came from hegel_run_start and is freed exactly once;
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_free(ctx, self.raw)
        }));
    }
}

/// A libhegel test-case handle plus the per-primitive operations the frontend
/// drives it with.
///
/// Every `CTestCase` owns an independent libhegel handle — from
/// [`from_blob`](CTestCase::from_blob), [`next_test_case`](RunHandle::next_test_case),
/// or [`clone_handle`](CTestCase::clone_handle) — and drops its reference via
/// `hegel_test_case_free` on drop; the shared test case is released once its
/// last reference is gone. Frontend code that needs several owners of *one*
/// handle (the lifecycle and the body's `TestCase`, a `TestCase` and its
/// children) shares it behind an `Arc<CTestCase>`.
pub(crate) struct CTestCase {
    raw: *mut hegel_c::HegelTestCase,
}

// SAFETY: libhegel guards every handle with its own lock — the draw primitives
// refuse concurrent use of a single handle (`HEGEL_E_CONCURRENT_USE`) and
// `hegel_mark_complete` waits for an in-flight operation — so a handle is
// sound to move between threads and to share by reference; clones (separate
// handles) carry their own locks.
unsafe impl Send for CTestCase {}
unsafe impl Sync for CTestCase {}

impl CTestCase {
    /// Build a standalone test case that replays a base64 failure blob. Owned
    /// by the caller (freed on drop). Returns `Err` with libhegel's diagnostic
    /// if the blob is null/non-UTF-8/undecodable.
    pub(crate) fn from_blob(settings: &SettingsHandle, blob: &str) -> Result<Self, String> {
        let c_blob = cstring_lossy(blob);
        let mut raw: *mut hegel_c::HegelTestCase = ptr::null_mut();
        // SAFETY: settings is live; c_blob is a valid NUL-terminated string;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_test_case_from_blob(ctx, settings.as_ptr(), c_blob.as_ptr(), &mut raw)
        });
        if rc != hegel_result_t::HEGEL_OK {
            return Err(last_error_string());
        }
        Ok(CTestCase { raw })
    }

    /// Clone this handle via `hegel_test_case_clone`, yielding a new libhegel
    /// handle onto the same underlying test case. Clones have independent
    /// per-handle locks, so two of them may draw concurrently; this is how a
    /// `TestCase` clone is moved to another thread. The clone holds its own
    /// reference to the shared test case and is freed independently on drop;
    /// the test case is released once its last handle is freed.
    pub(crate) fn clone_handle(&self) -> CTestCase {
        let mut raw: *mut hegel_c::HegelTestCase = ptr::null_mut();
        // SAFETY: self.raw is a live handle; &mut raw is a valid out-param.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_test_case_clone(ctx, self.raw, &mut raw)
        }));
        CTestCase { raw }
    }

    /// Generate a CBOR value for `schema_cbor`, returning a fresh copy of the
    /// bytes (libhegel's buffer is invalidated by the next call on this
    /// handle, so we copy immediately).
    pub(crate) fn generate(&self, schema_cbor: &[u8]) -> Result<Vec<u8>, hegel_result_t> {
        let mut out_ptr: *const u8 = ptr::null();
        let mut out_len: usize = 0;
        // SAFETY: schema bytes + out params are valid; on HEGEL_OK libhegel
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate(
                ctx,
                self.raw,
                schema_cbor.as_ptr(),
                schema_cbor.len(),
                &mut out_ptr,
                &mut out_len,
            )
        });
        if rc != hegel_result_t::HEGEL_OK {
            return Err(rc);
        }
        // SAFETY: on success out_ptr/out_len describe a valid borrowed buffer.
        let bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        Ok(bytes.to_vec())
    }

    pub(crate) fn start_span(&self, label: u64) -> Result<(), hegel_result_t> {
        rc_to_unit(with_context(|ctx| unsafe {
            hegel_c::hegel_start_span(ctx, self.raw, label)
        }))
    }

    pub(crate) fn stop_span(&self, discard: bool) -> Result<(), hegel_result_t> {
        rc_to_unit(with_context(|ctx| unsafe {
            hegel_c::hegel_stop_span(ctx, self.raw, discard)
        }))
    }

    pub(crate) fn new_collection(
        &self,
        min_size: u64,
        max_size: Option<u64>,
    ) -> Result<i64, hegel_result_t> {
        let mut id: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_new_collection(
                ctx,
                self.raw,
                min_size,
                max_size.unwrap_or(u64::MAX),
                &mut id,
            )
        });
        rc_to_value(rc, id)
    }

    pub(crate) fn collection_more(&self, collection_id: i64) -> Result<bool, hegel_result_t> {
        let mut more = false;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_collection_more(ctx, self.raw, collection_id, &mut more)
        });
        rc_to_value(rc, more)
    }

    pub(crate) fn collection_reject(
        &self,
        collection_id: i64,
        why: Option<&str>,
    ) -> Result<(), hegel_result_t> {
        let c_why = why.map(cstring_lossy);
        let why_ptr = c_why.as_ref().map_or(ptr::null(), |c| c.as_ptr());
        rc_to_unit(with_context(|ctx| unsafe {
            hegel_c::hegel_collection_reject(ctx, self.raw, collection_id, why_ptr)
        }))
    }

    pub(crate) fn new_pool(&self) -> Result<i64, hegel_result_t> {
        let mut id: i64 = 0;
        let rc = with_context(|ctx| unsafe { hegel_c::hegel_new_pool(ctx, self.raw, &mut id) });
        rc_to_value(rc, id)
    }

    pub(crate) fn pool_add(&self, pool_id: i64) -> Result<i64, hegel_result_t> {
        let mut id: i64 = 0;
        let rc =
            with_context(|ctx| unsafe { hegel_c::hegel_pool_add(ctx, self.raw, pool_id, &mut id) });
        rc_to_value(rc, id)
    }

    pub(crate) fn pool_generate(&self, pool_id: i64, consume: bool) -> Result<i64, hegel_result_t> {
        let mut id: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_pool_generate(ctx, self.raw, pool_id, consume, &mut id)
        });
        rc_to_value(rc, id)
    }

    pub(crate) fn new_state_machine(
        &self,
        rule_names: &[&str],
        invariant_names: &[&str],
    ) -> Result<i64, hegel_result_t> {
        let rule_cstrings: Vec<CString> = rule_names.iter().map(|s| cstring_lossy(s)).collect();
        let invariant_cstrings: Vec<CString> =
            invariant_names.iter().map(|s| cstring_lossy(s)).collect();
        let rule_ptrs: Vec<*const c_char> = rule_cstrings.iter().map(|c| c.as_ptr()).collect();
        let invariant_ptrs: Vec<*const c_char> =
            invariant_cstrings.iter().map(|c| c.as_ptr()).collect();
        let mut id: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_new_state_machine(
                ctx,
                self.raw,
                rule_ptrs.as_ptr(),
                rule_ptrs.len(),
                invariant_ptrs.as_ptr(),
                invariant_ptrs.len(),
                &mut id,
            )
        });
        rc_to_value(rc, id)
    }

    pub(crate) fn state_machine_next_rule(
        &self,
        state_machine_id: i64,
    ) -> Result<i64, hegel_result_t> {
        let mut out: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_state_machine_next_rule(ctx, self.raw, state_machine_id, &mut out)
        });
        rc_to_value(rc, out)
    }

    pub(crate) fn target(&self, score: f64, label: &str) -> Result<(), hegel_result_t> {
        let c_label = cstring_lossy(label);
        rc_to_unit(with_context(|ctx| unsafe {
            hegel_c::hegel_target(ctx, self.raw, score, c_label.as_ptr())
        }))
    }

    /// Report the test case's outcome. `origin` is supplied only for an
    /// interesting (failing) status; libhegel ignores it otherwise.
    pub(crate) fn mark_complete(
        &self,
        status: hegel_c::hegel_status_t,
        origin: Option<&str>,
    ) -> Result<(), hegel_result_t> {
        let c_origin = origin.map(cstring_lossy);
        let origin_ptr = c_origin.as_ref().map_or(ptr::null(), |c| c.as_ptr());
        rc_to_unit(with_context(|ctx| unsafe {
            hegel_c::hegel_mark_complete(ctx, self.raw, status, origin_ptr)
        }))
    }
}

impl Drop for CTestCase {
    fn drop(&mut self) {
        // SAFETY: every `CTestCase` is an independent libhegel handle this
        // frontend created (from_blob, next_test_case, or clone_handle) and is
        // freed exactly once here, dropping its reference to the test case.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_test_case_free(ctx, self.raw)
        }));
    }
}

/// Borrowed view of a finished run's aggregate result. Tied to its
/// [`RunHandle`] by the lifetime parameter so it cannot outlive the run.
pub(crate) struct RunResult<'run> {
    raw: *const hegel_c::HegelRunResult,
    _run: std::marker::PhantomData<&'run RunHandle>,
}

impl RunResult<'_> {
    pub(crate) fn status(&self) -> hegel_c::hegel_run_status_t {
        let mut status = hegel_c::hegel_run_status_t::HEGEL_RUN_STATUS_ERROR;
        // SAFETY: self.raw is borrowed from the run; &mut status is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_status(ctx, self.raw, &mut status)
        }));
        status
    }

    /// Run-level error message (failed health check, nondeterminism, engine
    /// panic), or `None` for a normal run.
    pub(crate) fn error(&self) -> Option<String> {
        let mut p: *const c_char = ptr::null();
        // SAFETY: self.raw is borrowed from the run; &mut p is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_error(ctx, self.raw, &mut p)
        }));
        cstr_opt(p)
    }

    pub(crate) fn failure_count(&self) -> usize {
        let mut count = 0;
        // SAFETY: self.raw is borrowed from the run; &mut count is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_failure_count(ctx, self.raw, &mut count)
        }));
        count
    }

    /// The `index`-th distinct failure, or `None` if out of range.
    pub(crate) fn failure(&self, index: usize) -> Option<Failure> {
        let mut f: *const hegel_c::HegelFailure = ptr::null();
        // SAFETY: self.raw is borrowed from the run; &mut f is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_failure(ctx, self.raw, index, &mut f)
        }));
        if f.is_null() {
            return None;
        }
        let mut blob: *const c_char = ptr::null();
        // SAFETY: f is borrowed from the run result; &mut blob is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_failure_reproduction_blob(ctx, f, &mut blob)
        }));
        Some(Failure {
            reproduce_blob: cstr_opt(blob),
        })
    }
}

/// A distinct failure read out of a finished run.
///
/// The client needs only the reproduce blob: it replays the blob to produce
/// the diagnostic and re-raise the test's own panic.
pub(crate) struct Failure {
    pub(crate) reproduce_blob: Option<String>,
}

fn rc_to_unit(rc: hegel_result_t) -> Result<(), hegel_result_t> {
    if rc == hegel_result_t::HEGEL_OK {
        Ok(())
    } else {
        Err(rc)
    }
}

/// Require that a libhegel call the frontend makes with controlled inputs
/// returned `HEGEL_OK`. These calls — the `hegel_settings_set_*` configuration
/// setters, the result/failure getters, and the `hegel_*_free` teardown calls —
/// cannot legitimately fail given the arguments the frontend passes, so an
/// unexpected non-OK code is an internal error. It is raised via
/// [`raise_for_rc`](crate::test_case::raise_for_rc); reached from a `Drop` impl
/// that would abort the process, which is acceptable for an invariant that
/// should never trip.
fn require_ok(rc: hegel_result_t) {
    rc_to_unit(rc).unwrap_or_else(|rc| crate::test_case::raise_for_rc(rc));
}

fn rc_to_value<T>(rc: hegel_result_t, value: T) -> Result<T, hegel_result_t> {
    if rc == hegel_result_t::HEGEL_OK {
        Ok(value)
    } else {
        Err(rc)
    }
}

/// Copy a (possibly null) borrowed C string into an owned `String`, or `None`
/// if the pointer is null.
fn cstr_opt(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }
}

fn map_mode(mode: Mode) -> hegel_c::hegel_mode_t {
    match mode {
        Mode::TestRun => hegel_c::hegel_mode_t::HEGEL_MODE_TEST_RUN,
        Mode::SingleTestCase => hegel_c::hegel_mode_t::HEGEL_MODE_SINGLE_TEST_CASE,
    }
}

fn map_verbosity(v: Verbosity) -> hegel_c::hegel_verbosity_t {
    match v {
        Verbosity::Quiet => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_QUIET,
        Verbosity::Normal => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_NORMAL,
        Verbosity::Verbose => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_VERBOSE,
        Verbosity::Debug => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG,
    }
}

fn map_backend(backend: Option<Backend>) -> hegel_c::hegel_backend_t {
    match backend {
        None => hegel_c::hegel_backend_t::HEGEL_BACKEND_AUTO,
        Some(Backend::Default) => hegel_c::hegel_backend_t::HEGEL_BACKEND_DEFAULT,
        Some(Backend::Urandom) => hegel_c::hegel_backend_t::HEGEL_BACKEND_URANDOM,
    }
}

fn phases_bitmask(phases: &[Phase]) -> u32 {
    let mut mask = 0;
    for phase in phases {
        mask |= match phase {
            Phase::Explicit => hegel_c::hegel_phase_t::HEGEL_PHASE_EXPLICIT as u32,
            Phase::Reuse => hegel_c::hegel_phase_t::HEGEL_PHASE_REUSE as u32,
            Phase::Generate => hegel_c::hegel_phase_t::HEGEL_PHASE_GENERATE as u32,
            Phase::Target => hegel_c::hegel_phase_t::HEGEL_PHASE_TARGET as u32,
            Phase::Shrink => hegel_c::hegel_phase_t::HEGEL_PHASE_SHRINK as u32,
        };
    }
    mask
}

fn health_check_bitmask(checks: &[HealthCheck]) -> u32 {
    let mut mask = 0;
    for check in checks {
        mask |= match check {
            HealthCheck::FilterTooMuch => {
                hegel_c::hegel_health_check_t::HEGEL_HC_FILTER_TOO_MUCH as u32
            }
            HealthCheck::TooSlow => hegel_c::hegel_health_check_t::HEGEL_HC_TOO_SLOW as u32,
            HealthCheck::TestCasesTooLarge => {
                hegel_c::hegel_health_check_t::HEGEL_HC_TEST_CASES_TOO_LARGE as u32
            }
            HealthCheck::LargeInitialTestCase => {
                hegel_c::hegel_health_check_t::HEGEL_HC_LARGE_INITIAL_TEST_CASE as u32
            }
        };
    }
    mask
}

#[cfg(test)]
#[path = "../tests/embedded/ffi_tests.rs"]
mod tests;
