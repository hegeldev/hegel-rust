//! The libhegel C-ABI boundary.
//!
//! hegeltest drives the engine the same way every other language binding
//! does: through the `hegel_*` C functions exported by the `hegel-c` crate
//! (lib name `hegel_c`), passing CBOR bytes and opaque handles and reading
//! back `c_int` error codes. This module is the single place that touches
//! those raw functions; the rest of the frontend works against the safe
//! wrappers here.
//!
//! The wrappers deliberately do *not* know about hegeltest's control-flow
//! unwinds: the per-test-case methods return `Result<_, c_int>` and leave it
//! to [`crate::test_case`] to translate a non-`HEGEL_OK` code into the right
//! [`crate::control`] payload (a `StopTest` / `AssumeFailed` / invalid-argument
//! unwind). Keeping that split means the unsafe boundary stays small and the
//! control-flow policy stays with the test lifecycle.

// TRANSIENT: this module is validated by its embedded tests but not yet wired
// into the lib's non-test code — the frontend rewrite (test_case / run_lifecycle
// / runner / stateful) consumes every wrapper here in the immediately-following
// commits, at which point this allow is removed and any genuinely-unused item
// would surface.
#![allow(dead_code)]

use crate::runner::{Backend, Database, HealthCheck, Mode, Phase, Settings, Verbosity};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;

/// The most recent error message libhegel recorded on this thread, or an
/// empty string if the last call succeeded. libhegel's buffer is borrowed
/// and invalidated by the next call on this thread, so we copy it out
/// immediately.
pub(crate) fn last_error_string() -> String {
    // SAFETY: returns a borrowed pointer into a thread-local buffer valid
    // until the next libhegel call on this thread; we copy before returning.
    let p = hegel_c::hegel_last_error_message();
    if p.is_null() {
        // libhegel's thread-local error buffer is a CString whose pointer is
        // never null; this guard exists for FFI soundness (CStr::from_ptr is UB
        // on null), so the branch is unreachable in practice but must stay a
        // guard rather than become a panic.
        return String::new(); // nocov
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Build a NUL-terminated C string, replacing any interior NUL (which a
/// `CString` cannot represent) with the Unicode replacement character so the
/// value still round-trips to a diagnostic rather than being dropped.
fn cstring_lossy(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|_| CString::new(s.replace('\0', "\u{FFFD}")).unwrap())
}

// ─── Settings handle ─────────────────────────────────────────────────────────

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
        // SAFETY: hegel_settings_new never returns null; every setter is a
        // documented no-op on a null handle and we pass our own non-null one.
        let raw = hegel_c::hegel_settings_new();
        unsafe {
            hegel_c::hegel_settings_mode(raw, map_mode(settings.mode));
            hegel_c::hegel_settings_test_cases(raw, settings.test_cases);
            hegel_c::hegel_settings_verbosity(raw, map_verbosity(settings.verbosity));
            match settings.seed {
                Some(seed) => hegel_c::hegel_settings_seed(raw, seed, true),
                None => hegel_c::hegel_settings_seed(raw, 0, false),
            }
            hegel_c::hegel_settings_derandomize(raw, settings.derandomize);
            hegel_c::hegel_settings_report_multiple_failures(
                raw,
                settings.report_multiple_failures,
            );
            match &settings.database {
                // Empty string disables the database; a path selects it; Unset
                // leaves libhegel's default in place (don't call the setter).
                Database::Disabled => {
                    let empty = CString::new("").unwrap();
                    hegel_c::hegel_settings_database(raw, empty.as_ptr());
                }
                Database::Path(path) => {
                    let c = cstring_lossy(path);
                    hegel_c::hegel_settings_database(raw, c.as_ptr());
                }
                Database::Unset => {}
            }
            if let Some(key) = database_key {
                let c = cstring_lossy(key);
                hegel_c::hegel_settings_database_key(raw, c.as_ptr());
            }
            hegel_c::hegel_settings_phases(raw, phases_bitmask(&settings.phases));
            hegel_c::hegel_settings_suppress_health_check(
                raw,
                health_check_bitmask(&settings.suppress_health_check),
            );
            hegel_c::hegel_settings_backend(raw, map_backend(settings.backend));
        }
        SettingsHandle { raw }
    }

    pub(crate) fn as_ptr(&self) -> *const hegel_c::HegelSettings {
        self.raw
    }
}

impl Drop for SettingsHandle {
    fn drop(&mut self) {
        // SAFETY: `raw` came from hegel_settings_new and is freed exactly once.
        unsafe { hegel_c::hegel_settings_free(self.raw) };
    }
}

// ─── Run handle ──────────────────────────────────────────────────────────────

/// Owns a `*mut HegelRun` and frees it on drop (which aborts and joins the
/// engine worker if the run was not drained to completion).
pub(crate) struct RunHandle {
    raw: *mut hegel_c::HegelRun,
}

impl RunHandle {
    /// Start a run. Returns `Err` with libhegel's diagnostic if the engine
    /// could not be started.
    pub(crate) fn start(settings: &SettingsHandle) -> Result<Self, String> {
        // SAFETY: settings.as_ptr() is a live, non-null handle.
        let raw = unsafe { hegel_c::hegel_run_start(settings.as_ptr()) };
        if raw.is_null() {
            // settings is always a live handle, so hegel_run_start returns null
            // only on OS worker-thread spawn failure: a real but unprovokable
            // resource-exhaustion path.
            return Err(last_error_string()); // nocov
        }
        Ok(RunHandle { raw })
    }

    /// Pull the next test case the engine wants to run, or `None` when the run
    /// is finished. The returned handle is owned by the run (freed when the
    /// run is freed), so it is wrapped with `owned = false`.
    pub(crate) fn next_test_case(&self) -> Option<CTestCase> {
        // SAFETY: self.raw is a live run handle; libhegel blocks until the next
        // case or returns null at completion.
        let raw = unsafe { hegel_c::hegel_next_test_case(self.raw) };
        if raw.is_null() {
            None
        } else {
            Some(CTestCase { raw, owned: false })
        }
    }

    /// Read the aggregate result. Borrowed from the run; valid until the run
    /// is dropped.
    pub(crate) fn result(&self) -> RunResult<'_> {
        // SAFETY: called after the pull loop drained; libhegel returns a
        // borrowed pointer valid for the run's lifetime.
        let raw = unsafe { hegel_c::hegel_run_result(self.raw) };
        RunResult {
            raw,
            _run: std::marker::PhantomData,
        }
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        // SAFETY: `raw` came from hegel_run_start and is freed exactly once;
        // hegel_run_free tolerates an undrained run (aborts + joins the worker).
        unsafe { hegel_c::hegel_run_free(self.raw) };
    }
}

// ─── Test case handle ────────────────────────────────────────────────────────

/// A libhegel test-case handle plus the per-primitive operations the frontend
/// drives it with. Either borrowed from a run (`owned = false`, freed by the
/// run) or owned by us when produced from a replay blob (`owned = true`).
pub(crate) struct CTestCase {
    raw: *mut hegel_c::HegelTestCase,
    owned: bool,
}

// SAFETY: libhegel's per-test-case primitives are single-threaded *per handle*;
// the frontend serializes every call through `TestCase`'s shared mutex, exactly
// as the previous `Box<dyn DataSource + Send + Sync>` did. The raw pointer
// carries no thread affinity of its own.
unsafe impl Send for CTestCase {}
unsafe impl Sync for CTestCase {}

impl CTestCase {
    /// Build a standalone test case that replays a base64 failure blob. Owned
    /// by the caller (freed on drop). Returns `Err` with libhegel's diagnostic
    /// if the blob is null/non-UTF-8/undecodable.
    pub(crate) fn from_blob(settings: &SettingsHandle, blob: &str) -> Result<Self, String> {
        let c_blob = cstring_lossy(blob);
        // SAFETY: settings is live; c_blob is a valid NUL-terminated string.
        let raw = unsafe { hegel_c::hegel_test_case_from_blob(settings.as_ptr(), c_blob.as_ptr()) };
        if raw.is_null() {
            return Err(last_error_string());
        }
        Ok(CTestCase { raw, owned: true })
    }

    /// Generate a CBOR value for `schema_cbor`, returning a fresh copy of the
    /// bytes (libhegel's buffer is invalidated by the next call on this
    /// handle, so we copy immediately).
    pub(crate) fn generate(&self, schema_cbor: &[u8]) -> Result<Vec<u8>, c_int> {
        let mut out_ptr: *const u8 = ptr::null();
        let mut out_len: usize = 0;
        // SAFETY: schema bytes + out params are valid; on HEGEL_OK libhegel
        // writes a borrowed (ptr, len) we copy before any further call.
        let rc = unsafe {
            hegel_c::hegel_generate(
                self.raw,
                schema_cbor.as_ptr(),
                schema_cbor.len(),
                &mut out_ptr,
                &mut out_len,
            )
        };
        if rc != hegel_c::HEGEL_OK {
            return Err(rc);
        }
        // SAFETY: on success out_ptr/out_len describe a valid borrowed buffer.
        let bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        Ok(bytes.to_vec())
    }

    pub(crate) fn start_span(&self, label: u64) -> Result<(), c_int> {
        rc_to_unit(unsafe { hegel_c::hegel_start_span(self.raw, label) })
    }

    pub(crate) fn stop_span(&self, discard: bool) -> Result<(), c_int> {
        rc_to_unit(unsafe { hegel_c::hegel_stop_span(self.raw, discard) })
    }

    pub(crate) fn new_collection(
        &self,
        min_size: u64,
        max_size: Option<u64>,
    ) -> Result<i64, c_int> {
        let mut id: i64 = 0;
        let rc = unsafe {
            hegel_c::hegel_new_collection(self.raw, min_size, max_size.unwrap_or(u64::MAX), &mut id)
        };
        rc_to_value(rc, id)
    }

    pub(crate) fn collection_more(&self, collection_id: i64) -> Result<bool, c_int> {
        let mut more = false;
        let rc = unsafe { hegel_c::hegel_collection_more(self.raw, collection_id, &mut more) };
        rc_to_value(rc, more)
    }

    pub(crate) fn collection_reject(
        &self,
        collection_id: i64,
        why: Option<&str>,
    ) -> Result<(), c_int> {
        let c_why = why.map(cstring_lossy);
        let why_ptr = c_why.as_ref().map_or(ptr::null(), |c| c.as_ptr());
        rc_to_unit(unsafe { hegel_c::hegel_collection_reject(self.raw, collection_id, why_ptr) })
    }

    pub(crate) fn new_pool(&self) -> Result<i64, c_int> {
        let mut id: i64 = 0;
        let rc = unsafe { hegel_c::hegel_new_pool(self.raw, &mut id) };
        rc_to_value(rc, id)
    }

    pub(crate) fn pool_add(&self, pool_id: i64) -> Result<i64, c_int> {
        let mut id: i64 = 0;
        let rc = unsafe { hegel_c::hegel_pool_add(self.raw, pool_id, &mut id) };
        rc_to_value(rc, id)
    }

    pub(crate) fn pool_generate(&self, pool_id: i64, consume: bool) -> Result<i64, c_int> {
        let mut id: i64 = 0;
        let rc = unsafe { hegel_c::hegel_pool_generate(self.raw, pool_id, consume, &mut id) };
        rc_to_value(rc, id)
    }

    pub(crate) fn target(&self, score: f64, label: &str) -> Result<(), c_int> {
        let c_label = cstring_lossy(label);
        rc_to_unit(unsafe { hegel_c::hegel_target(self.raw, score, c_label.as_ptr()) })
    }

    /// Report the test case's outcome. `origin` is supplied only for an
    /// interesting (failing) status; libhegel ignores it otherwise.
    pub(crate) fn mark_complete(
        &self,
        status: hegel_c::hegel_status_t,
        origin: Option<&str>,
    ) -> Result<(), c_int> {
        let c_origin = origin.map(cstring_lossy);
        let origin_ptr = c_origin.as_ref().map_or(ptr::null(), |c| c.as_ptr());
        rc_to_unit(unsafe { hegel_c::hegel_mark_complete(self.raw, status, origin_ptr) })
    }

    /// Whether this is the engine's final replay of a minimal counterexample
    /// (used to gate verbose draw output to the counterexample only).
    pub(crate) fn is_final_replay(&self) -> bool {
        unsafe { hegel_c::hegel_test_case_is_final_replay(self.raw) }
    }
}

impl Drop for CTestCase {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: a `owned` handle came from from_blob and is ours to free
            // exactly once. Run-owned handles (owned = false) are freed by the
            // run and must not be touched here.
            unsafe { hegel_c::hegel_test_case_free(self.raw) };
        }
    }
}

// ─── Run result (borrowed from a RunHandle) ──────────────────────────────────

/// Borrowed view of a finished run's aggregate result. Tied to its
/// [`RunHandle`] by the lifetime parameter so it cannot outlive the run.
pub(crate) struct RunResult<'run> {
    raw: *const hegel_c::HegelRunResult,
    _run: std::marker::PhantomData<&'run RunHandle>,
}

impl RunResult<'_> {
    pub(crate) fn status(&self) -> hegel_c::hegel_run_status_t {
        unsafe { hegel_c::hegel_run_result_status(self.raw) }
    }

    /// Run-level error message (failed health check, nondeterminism, engine
    /// panic), or `None` for a normal run.
    pub(crate) fn error(&self) -> Option<String> {
        let p = unsafe { hegel_c::hegel_run_result_error(self.raw) };
        cstr_opt(p)
    }

    pub(crate) fn failure_count(&self) -> usize {
        unsafe { hegel_c::hegel_run_result_failure_count(self.raw) }
    }

    /// The `index`-th distinct failure, or `None` if out of range.
    pub(crate) fn failure(&self, index: usize) -> Option<Failure> {
        let f = unsafe { hegel_c::hegel_run_result_failure(self.raw, index) };
        if f.is_null() {
            return None;
        }
        Some(Failure {
            panic_message: cstr_opt(unsafe { hegel_c::hegel_failure_panic_message(f) })
                .unwrap_or_default(),
            origin: cstr_opt(unsafe { hegel_c::hegel_failure_origin(f) }).unwrap_or_default(),
            reproduce_blob: cstr_opt(unsafe { hegel_c::hegel_failure_reproduction_blob(f) }),
        })
    }
}

/// A distinct failure read out of a finished run.
pub(crate) struct Failure {
    pub(crate) panic_message: String,
    pub(crate) origin: String,
    pub(crate) reproduce_blob: Option<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn rc_to_unit(rc: c_int) -> Result<(), c_int> {
    if rc == hegel_c::HEGEL_OK {
        Ok(())
    } else {
        Err(rc)
    }
}

fn rc_to_value<T>(rc: c_int, value: T) -> Result<T, c_int> {
    if rc == hegel_c::HEGEL_OK {
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
            Phase::Explicit => hegel_c::HEGEL_PHASE_EXPLICIT,
            Phase::Reuse => hegel_c::HEGEL_PHASE_REUSE,
            Phase::Generate => hegel_c::HEGEL_PHASE_GENERATE,
            Phase::Target => hegel_c::HEGEL_PHASE_TARGET,
            Phase::Shrink => hegel_c::HEGEL_PHASE_SHRINK,
        };
    }
    mask
}

fn health_check_bitmask(checks: &[HealthCheck]) -> u32 {
    let mut mask = 0;
    for check in checks {
        mask |= match check {
            HealthCheck::FilterTooMuch => hegel_c::HEGEL_HC_FILTER_TOO_MUCH,
            HealthCheck::TooSlow => hegel_c::HEGEL_HC_TOO_SLOW,
            HealthCheck::TestCasesTooLarge => hegel_c::HEGEL_HC_TEST_CASES_TOO_LARGE,
            HealthCheck::LargeInitialTestCase => hegel_c::HEGEL_HC_LARGE_INITIAL_TEST_CASE,
        };
    }
    mask
}

#[cfg(test)]
#[path = "../tests/embedded/ffi_tests.rs"]
mod tests;
