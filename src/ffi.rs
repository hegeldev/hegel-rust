//! The libhegel C-ABI boundary.
//!
//! hegeltest drives the engine the same way every other language binding
//! does: through the `hegel_*` C functions exported by the `hegel-c` crate
//! (lib name `hegel_c`), passing typed values and opaque handles and reading
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
use crate::test_case::OutputSink;
use hegel_c::hegel_result_t;
use std::ffi::{CStr, CString, c_void};
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

/// Run a libhegel free function from a `Drop` impl.
///
/// A handle a user caches in their own thread-local can be dropped during
/// thread teardown after this thread's [`CONTEXT`] has already been
/// destroyed (thread-local destructors run last-initialized-first, and the
/// user's slot may well be initialized before our context). `LocalKey::with`
/// panics on a destroyed key, and a panic inside a thread-local destructor
/// aborts the process — so this uses `try_with` and skips the free when the
/// context is gone. The engine-side allocation leaks at thread exit, which
/// is harmless by comparison.
fn free_on_drop(f: impl FnOnce(*mut hegel_c::HegelContext) -> hegel_c::hegel_result_t) {
    let _ = CONTEXT.try_with(|c| require_ok(f(c.as_ptr())));
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

/// Convert an engine-produced string buffer into an owned `String`, raising
/// an internal error if the engine violated its guarantee of returning valid
/// UTF-8.
fn string_from_engine_bytes(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| {
        crate::control::hegel_internal_error!("libhegel returned invalid UTF-8: {e}")
    })
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
            // out-parameter.
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
                require_ok(hegel_c::hegel_settings_set_nondeterministic(
                    ctx,
                    raw,
                    settings.nondeterministic,
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
        free_on_drop(|ctx| unsafe { hegel_c::hegel_settings_free(ctx, self.raw) });
    }
}

/// Engine-output trampoline passed to `hegel_run_start` /
/// `hegel_test_case_from_blob`: `user_data` points at the [`OutputSink`] the
/// run resolved at start, and each engine output line is forwarded to it. The
/// engine invokes this while it runs between test cases; the sink is
/// `Send + Sync`, and
/// the pointee stays alive for as long as the engine can emit — owned by the
/// [`RunHandle`] for a run, borrowed across the creating call for a blob
/// replay (whose only line is emitted during it).
unsafe extern "C" fn engine_output_trampoline(
    user_data: *mut c_void,
    line: *const c_char,
    len: usize,
) {
    let sink = unsafe { &*user_data.cast::<OutputSink>() };
    let bytes = unsafe { std::slice::from_raw_parts(line.cast::<u8>(), len) };
    sink(&String::from_utf8_lossy(bytes));
}

/// The `(callback, user_data)` pair to pass to a creation call for output
/// going to the [`OutputSink`] at `sink_ptr`, or `(None, null)` to leave
/// output on stderr when `sink_ptr` is `None`. The pointee must stay valid
/// for as long as the engine can emit through it (the caller's concern).
fn output_args(
    sink_ptr: Option<*const OutputSink>,
) -> (hegel_c::hegel_output_callback_t, *mut c_void) {
    match sink_ptr {
        Some(p) => (Some(engine_output_trampoline), p.cast_mut().cast()),
        None => (None, ptr::null_mut()),
    }
}

/// Owns a `*mut HegelRun` and frees it on drop (which aborts and joins the
/// engine worker if the run was not drained to completion).
pub(crate) struct RunHandle {
    raw: *mut hegel_c::HegelRun,
    /// The engine-output sink registered for this run, or `None` when the
    /// run writes to stderr. The engine worker holds the raw `user_data`
    /// pointer to this allocation for the life of the run, so it is freed
    /// only in `Drop`, after `hegel_run_free` has joined the worker.
    output: Option<*mut OutputSink>,
}

impl RunHandle {
    /// Start a run whose engine output goes to `sink` (stderr when `None`).
    /// Returns `Err` with libhegel's diagnostic if the engine could not be
    /// started.
    pub(crate) fn start(
        settings: &SettingsHandle,
        sink: Option<&OutputSink>,
    ) -> Result<Self, String> {
        let output = sink.map(|s| Box::into_raw(Box::new(s.clone())));
        let (callback, user_data) = output_args(output.map(|p| p.cast_const()));
        let mut raw: *mut hegel_c::HegelRun = ptr::null_mut();
        // SAFETY: settings.as_ptr() is a live, non-null handle; &mut raw is a
        // valid out-parameter. The trampoline contract (thread-safe callback,
        // pointee outlives the engine's emissions) is upheld by holding the
        // sink box in `output` until Drop, after the worker is joined.
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_run_start(ctx, settings.as_ptr(), callback, user_data, &mut raw)
        });
        // Construct the handle before checking rc so the error path (raw is
        // still null, which hegel_run_free accepts) releases the sink box.
        let run = RunHandle { raw, output };
        if rc != hegel_result_t::HEGEL_OK {
            return Err(last_error_string()); // nocov
        }
        Ok(run)
    }

    /// Pull the next test case the engine wants to run, or `None` when the run
    /// is finished. The returned handle holds its own reference to the test
    /// case (the run keeps a separate reference internally), so the frontend
    /// owns it and frees it on drop.
    pub(crate) fn next_test_case(&self) -> Option<CTestCase> {
        let mut raw: *mut hegel_c::HegelTestCase = ptr::null_mut();
        // SAFETY: self.raw is a live run handle; libhegel blocks until the next
        // test case is available or the run completes.
        let rc =
            with_context(|ctx| unsafe { hegel_c::hegel_next_test_case(ctx, self.raw, &mut raw) });
        require_ok(rc);
        if raw.is_null() {
            None
        } else {
            Some(CTestCase { raw })
        }
    }

    /// Read the aggregate result as an owned snapshot, independent of the
    /// run's lifetime; freed on drop.
    pub(crate) fn result(&self) -> RunResult {
        let mut raw: *mut hegel_c::HegelRunResult = ptr::null_mut();
        // SAFETY: called after the pull loop drained; &mut raw is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result(ctx, self.raw, &mut raw)
        }));
        RunResult { raw }
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        // SAFETY: `raw` came from hegel_run_start and is freed exactly once;
        free_on_drop(|ctx| unsafe { hegel_c::hegel_run_free(ctx, self.raw) });
        if let Some(p) = self.output {
            // SAFETY: hegel_run_free joined the engine worker above, so
            // nothing can invoke the trampoline with this pointer any more;
            // the box came from Box::into_raw in `start` and is
            // reconstituted and freed exactly once.
            drop(unsafe { Box::from_raw(p) });
        }
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
    /// Build a standalone test case that replays a base64 failure blob, with
    /// engine output (the debug-verbosity replay trace) going to `sink`
    /// (stderr when `None`). Owned by the caller (freed on drop). Returns
    /// `Err` with libhegel's diagnostic if the blob is
    /// null/non-UTF-8/undecodable.
    pub(crate) fn from_blob(
        settings: &SettingsHandle,
        blob: &str,
        sink: Option<&OutputSink>,
    ) -> Result<Self, String> {
        let c_blob = cstring_lossy(blob);
        let (callback, user_data) = output_args(sink.map(ptr::from_ref));
        let mut raw: *mut hegel_c::HegelTestCase = ptr::null_mut();
        // SAFETY: settings is live; c_blob is a valid NUL-terminated string.
        // The blob-replay trace is emitted synchronously during this call, so
        // borrowing `sink` for its duration satisfies the trampoline contract.
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_test_case_from_blob(
                ctx,
                settings.as_ptr(),
                c_blob.as_ptr(),
                callback,
                user_data,
                &mut raw,
            )
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

    /// Draw an integer in `[min_value, max_value]` (both within `i64`).
    pub(crate) fn generate_integer(
        &self,
        min_value: i64,
        max_value: i64,
    ) -> Result<i64, hegel_result_t> {
        let mut out: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_integer(ctx, self.raw, min_value, max_value, &mut out)
        });
        rc_to_value(rc, out)
    }

    /// Draw an integer with bounds given as two's-complement little-endian
    /// byte encodings, returning the drawn value's encoding sign-extended to
    /// fill a 17-byte buffer (wide enough for any `i128` or `u128` value).
    /// libhegel sign-fills the buffer beyond the minimal encoding, so the
    /// whole array reads directly as a fixed-width value.
    pub(crate) fn generate_integer_big(
        &self,
        min_value: &[u8],
        max_value: &[u8],
    ) -> Result<[u8; 17], hegel_result_t> {
        let mut out = [0u8; 17];
        let mut out_len: usize = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_integer_big(
                ctx,
                self.raw,
                min_value.as_ptr(),
                min_value.len(),
                max_value.as_ptr(),
                max_value.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut out_len,
            )
        });
        if rc != hegel_result_t::HEGEL_OK {
            return Err(rc);
        }
        Ok(out)
    }

    /// Draw a float according to the full spec libhegel accepts.
    pub(crate) fn generate_float(
        &self,
        width: u32,
        min_value: f64,
        max_value: f64,
        allow_nan: bool,
        allow_infinity: bool,
        exclude_min: bool,
        exclude_max: bool,
        smallest_nonzero_magnitude: f64,
    ) -> Result<f64, hegel_result_t> {
        let mut out: f64 = 0.0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_float(
                ctx,
                self.raw,
                width,
                min_value,
                max_value,
                allow_nan,
                allow_infinity,
                exclude_min,
                exclude_max,
                smallest_nonzero_magnitude,
                &mut out,
            )
        });
        rc_to_value(rc, out)
    }

    /// Draw a boolean that is `true` with probability `p`.
    pub(crate) fn generate_boolean(&self, p: f64) -> Result<bool, hegel_result_t> {
        let mut out = false;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_boolean(ctx, self.raw, p, false, false, &mut out)
        });
        rc_to_value(rc, out)
    }

    /// Draw a byte string with length in `[min_size, max_size]`, copying the
    /// engine-allocated buffer into an owned `Vec` and freeing it.
    pub(crate) fn generate_bytes(
        &self,
        min_size: u64,
        max_size: u64,
    ) -> Result<Vec<u8>, hegel_result_t> {
        let mut result = hegel_c::hegel_generate_bytes_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_bytes(ctx, self.raw, min_size, max_size, &mut result)
        });
        if rc != hegel_result_t::HEGEL_OK {
            return Err(rc);
        }
        // SAFETY: on success the result holds a valid engine-allocated buffer
        // that this frontend owns; it is copied out and freed exactly once.
        let bytes = unsafe { std::slice::from_raw_parts(result.data, result.len) }.to_vec();
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_generate_bytes_result_free(ctx, &mut result)
        }));
        Ok(bytes)
    }

    /// Draw a string described by `generator`, copying the engine-allocated
    /// buffer into an owned `String` and freeing it.
    pub(crate) fn generate_string(
        &self,
        generator: &StringGenerator,
    ) -> Result<String, hegel_result_t> {
        let mut result = hegel_c::hegel_generate_string_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_string(ctx, self.raw, generator.raw, &mut result)
        });
        if rc != hegel_result_t::HEGEL_OK {
            return Err(rc);
        }
        // SAFETY: on success the result holds a valid engine-allocated UTF-8
        // buffer that this frontend owns; it is copied out and freed exactly
        // once.
        let bytes =
            unsafe { std::slice::from_raw_parts(result.data.cast::<u8>(), result.len) }.to_vec();
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_generate_string_result_free(ctx, &mut result)
        }));
        Ok(string_from_engine_bytes(bytes))
    }

    /// Draw a Gregorian calendar date in `[min, max]`.
    pub(crate) fn generate_date(
        &self,
        min: hegel_c::hegel_date_t,
        max: hegel_c::hegel_date_t,
    ) -> Result<hegel_c::hegel_date_t, hegel_result_t> {
        let mut out = min;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_date(ctx, self.raw, min, max, &mut out)
        });
        rc_to_value(rc, out)
    }

    /// Draw a time of day in `[min, max]`.
    pub(crate) fn generate_time(
        &self,
        min: hegel_c::hegel_time_t,
        max: hegel_c::hegel_time_t,
    ) -> Result<hegel_c::hegel_time_t, hegel_result_t> {
        let mut out = min;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_time(ctx, self.raw, min, max, &mut out)
        });
        rc_to_value(rc, out)
    }

    /// Draw a naive datetime in `[min, max]`.
    pub(crate) fn generate_datetime(
        &self,
        min: hegel_c::hegel_datetime_t,
        max: hegel_c::hegel_datetime_t,
    ) -> Result<hegel_c::hegel_datetime_t, hegel_result_t> {
        let mut out = min;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_datetime(ctx, self.raw, min, max, &mut out)
        });
        rc_to_value(rc, out)
    }

    /// Draw a UUID's 16 big-endian bytes, optionally forcing the RFC 4122
    /// version nibble.
    pub(crate) fn generate_uuid(&self, version: Option<u8>) -> Result<[u8; 16], hegel_result_t> {
        let mut out = [0u8; 16];
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_uuid(
                ctx,
                self.raw,
                version.unwrap_or(0),
                version.is_some(),
                out.as_mut_ptr(),
            )
        });
        rc_to_value(rc, out)
    }

    /// Draw an IPv4 address.
    pub(crate) fn generate_ipv4(&self) -> Result<std::net::Ipv4Addr, hegel_result_t> {
        let mut out = [0u8; 4];
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_ipv4(ctx, self.raw, out.as_mut_ptr())
        });
        rc_to_value(rc, std::net::Ipv4Addr::from(out))
    }

    /// Draw an IPv6 address.
    pub(crate) fn generate_ipv6(&self) -> Result<std::net::Ipv6Addr, hegel_result_t> {
        let mut out = [0u8; 16];
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_ipv6(ctx, self.raw, out.as_mut_ptr())
        });
        rc_to_value(rc, std::net::Ipv6Addr::from(out))
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

    /// Draw a concurrency level in `[1, max_value]`, weighted toward
    /// `max_value` (the engine owns the distribution).
    pub(crate) fn generate_concurrency(&self, max_value: i64) -> Result<i64, hegel_result_t> {
        let mut level: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_generate_concurrency(ctx, self.raw, max_value, &mut level)
        });
        rc_to_value(rc, level)
    }

    pub(crate) fn new_state_machine(
        &self,
        num_groups: usize,
        rule_names: &[&str],
        rule_groups: &[i64],
        invariant_names: &[&str],
        concurrency: i64,
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
                num_groups,
                rule_ptrs.as_ptr(),
                rule_groups.as_ptr(),
                rule_ptrs.len(),
                invariant_ptrs.as_ptr(),
                invariant_ptrs.len(),
                concurrency,
                &mut id,
            )
        });
        rc_to_value(rc, id)
    }

    /// Start the machine's next round, yielding the index of the round's
    /// current concurrency group; `None` once the engine has run enough
    /// rounds (`HEGEL_STATE_MACHINE_DONE`). Call on the root test-case
    /// handle at every join point, including before the first rule is
    /// requested.
    pub(crate) fn state_machine_next_group(
        &self,
        state_machine_id: i64,
    ) -> Result<Option<i64>, hegel_result_t> {
        let mut out: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_state_machine_next_group(ctx, self.raw, state_machine_id, &mut out)
        });
        let group = if out == hegel_c::HEGEL_STATE_MACHINE_DONE {
            None
        } else {
            Some(out)
        };
        rc_to_value(rc, group)
    }

    /// Ask the engine for the next rule for `thread_index` to run this
    /// round; `None` once the thread's round budget is exhausted
    /// (`HEGEL_STATE_MACHINE_DONE`) and it should wait for the join point.
    pub(crate) fn state_machine_next_rule(
        &self,
        state_machine_id: i64,
        thread_index: i64,
    ) -> Result<Option<i64>, hegel_result_t> {
        let mut out: i64 = 0;
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_state_machine_next_rule(
                ctx,
                self.raw,
                state_machine_id,
                thread_index,
                &mut out,
            )
        });
        let index = if out == hegel_c::HEGEL_STATE_MACHINE_DONE {
            None
        } else {
            Some(out)
        };
        rc_to_value(rc, index)
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
            hegel_c::hegel_mark_complete(ctx, self.raw, status as u32, origin_ptr)
        }))
    }
}

impl Drop for CTestCase {
    fn drop(&mut self) {
        // SAFETY: every `CTestCase` is an independent libhegel handle this
        // frontend created (from_blob, next_test_case, or clone_handle) and is
        // freed exactly once here, dropping its reference to the test case.
        free_on_drop(|ctx| unsafe { hegel_c::hegel_test_case_free(ctx, self.raw) });
    }
}

/// An owned libhegel string-generator handle (`hegel_string_generator_t`),
/// freed on drop.
///
/// Built by the constructor methods, each of which validates its parameters
/// eagerly and returns `Err` with libhegel's diagnostic on invalid input.
/// The handle is immutable after construction, so it may be shared across
/// test cases and threads; generators cache one per configuration.
pub(crate) struct StringGenerator {
    raw: *mut hegel_c::HegelStringGenerator,
}

// SAFETY: a string generator is immutable after construction — libhegel only
// ever reads through the pointer — so sharing across threads is sound.
unsafe impl Send for StringGenerator {}
unsafe impl Sync for StringGenerator {}

impl std::fmt::Debug for StringGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StringGenerator").finish_non_exhaustive()
    }
}

impl StringGenerator {
    /// Build a text generator over the alphabet described by the fields.
    /// `max_codepoint` of `None` means unconstrained.
    pub(crate) fn text(
        min_size: u64,
        max_size: u64,
        codec: Option<&str>,
        min_codepoint: u32,
        max_codepoint: Option<u32>,
        categories: Option<&[String]>,
        exclude_categories: Option<&[String]>,
        include_characters: Option<&str>,
        exclude_characters: Option<&str>,
    ) -> Result<Self, String> {
        let c_codec = codec.map(cstring_lossy);
        let c_categories: Option<Vec<CString>> =
            categories.map(|cats| cats.iter().map(|c| cstring_lossy(c)).collect());
        let c_exclude_categories: Option<Vec<CString>> =
            exclude_categories.map(|cats| cats.iter().map(|c| cstring_lossy(c)).collect());

        let category_ptrs: Option<Vec<*const c_char>> = c_categories
            .as_ref()
            .map(|cats| cats.iter().map(|c| c.as_ptr()).collect());
        let exclude_category_ptrs: Option<Vec<*const c_char>> = c_exclude_categories
            .as_ref()
            .map(|cats| cats.iter().map(|c| c.as_ptr()).collect());

        let mut raw: *mut hegel_c::HegelStringGenerator = ptr::null_mut();
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_string_generator_text(
                ctx,
                min_size,
                max_size,
                c_codec.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                min_codepoint,
                max_codepoint.unwrap_or(u32::MAX),
                category_ptrs.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
                category_ptrs.as_ref().map_or(0, |p| p.len()),
                exclude_category_ptrs
                    .as_ref()
                    .map_or(ptr::null(), |p| p.as_ptr()),
                exclude_category_ptrs.as_ref().map_or(0, |p| p.len()),
                include_characters.map_or(ptr::null(), |s| s.as_ptr()),
                include_characters.map_or(0, |s| s.len()),
                exclude_characters.map_or(ptr::null(), |s| s.as_ptr()),
                exclude_characters.map_or(0, |s| s.len()),
                &mut raw,
            )
        });
        Self::from_construction(rc, raw)
    }

    /// Build a regex generator; `alphabet` must be a text generator.
    pub(crate) fn regex(
        pattern: &str,
        fullmatch: bool,
        alphabet: Option<&StringGenerator>,
    ) -> Result<Self, String> {
        let c_pattern = cstring_lossy(pattern);
        let mut raw: *mut hegel_c::HegelStringGenerator = ptr::null_mut();
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_string_generator_regex(
                ctx,
                c_pattern.as_ptr(),
                fullmatch,
                alphabet.map_or(ptr::null(), |a| a.raw),
                &mut raw,
            )
        });
        Self::from_construction(rc, raw)
    }

    pub(crate) fn email() -> Result<Self, String> {
        let mut raw: *mut hegel_c::HegelStringGenerator = ptr::null_mut();
        let rc =
            with_context(|ctx| unsafe { hegel_c::hegel_string_generator_email(ctx, &mut raw) });
        Self::from_construction(rc, raw)
    }

    pub(crate) fn url() -> Result<Self, String> {
        let mut raw: *mut hegel_c::HegelStringGenerator = ptr::null_mut();
        let rc = with_context(|ctx| unsafe { hegel_c::hegel_string_generator_url(ctx, &mut raw) });
        Self::from_construction(rc, raw)
    }

    pub(crate) fn domain(max_length: u64) -> Result<Self, String> {
        let mut raw: *mut hegel_c::HegelStringGenerator = ptr::null_mut();
        let rc = with_context(|ctx| unsafe {
            hegel_c::hegel_string_generator_domain(ctx, max_length, &mut raw)
        });
        Self::from_construction(rc, raw)
    }

    fn from_construction(
        rc: hegel_result_t,
        raw: *mut hegel_c::HegelStringGenerator,
    ) -> Result<Self, String> {
        if rc != hegel_result_t::HEGEL_OK {
            return Err(last_error_string());
        }
        Ok(StringGenerator { raw })
    }
}

impl Drop for StringGenerator {
    fn drop(&mut self) {
        // SAFETY: `raw` came from a hegel_string_generator_* constructor and
        // is freed exactly once.
        free_on_drop(|ctx| unsafe { hegel_c::hegel_string_generator_free(ctx, self.raw) });
    }
}

/// Owned snapshot of a finished run's aggregate result, independent of the
/// [`RunHandle`] it was read from; released via `hegel_run_result_free` on
/// drop.
pub(crate) struct RunResult {
    raw: *mut hegel_c::HegelRunResult,
}

impl RunResult {
    pub(crate) fn status(&self) -> hegel_c::hegel_run_status_t {
        let mut status = hegel_c::hegel_run_status_t::HEGEL_RUN_STATUS_ERROR;
        // SAFETY: self.raw is this snapshot's live pointer; &mut status is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_status(ctx, self.raw, &mut status)
        }));
        status
    }

    /// Run-level error message (failed health check, nondeterminism, engine
    /// panic), or `None` for a normal run.
    pub(crate) fn error(&self) -> Option<String> {
        let mut p: *const c_char = ptr::null();
        // SAFETY: self.raw is this snapshot's live pointer; &mut p is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_error(ctx, self.raw, &mut p)
        }));
        cstr_opt(p)
    }

    pub(crate) fn failure_count(&self) -> usize {
        let mut count = 0;
        // SAFETY: self.raw is this snapshot's live pointer; &mut count is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_failure_count(ctx, self.raw, &mut count)
        }));
        count
    }

    /// The `index`-th distinct failure; `index` must be less than
    /// [`failure_count`](Self::failure_count) (libhegel rejects an
    /// out-of-range index). The blob is copied out and the libhegel failure
    /// snapshot released before returning.
    pub(crate) fn failure(&self, index: usize) -> Failure {
        let mut f: *mut hegel_c::HegelFailure = ptr::null_mut();
        // SAFETY: self.raw is this snapshot's live pointer; &mut f is valid.
        require_ok(with_context(|ctx| unsafe {
            hegel_c::hegel_run_result_failure(ctx, self.raw, index, &mut f)
        }));
        let mut blob: *const c_char = ptr::null();
        // SAFETY: f is the failure snapshot allocated above; it is freed
        // exactly once, after the blob has been copied out by cstr_opt.
        let reproduce_blob = with_context(|ctx| unsafe {
            require_ok(hegel_c::hegel_failure_reproduction_blob(ctx, f, &mut blob));
            let reproduce_blob = cstr_opt(blob);
            require_ok(hegel_c::hegel_failure_free(ctx, f));
            reproduce_blob
        });
        Failure { reproduce_blob }
    }
}

impl Drop for RunResult {
    fn drop(&mut self) {
        // SAFETY: `raw` came from hegel_run_result and is freed exactly once.
        free_on_drop(|ctx| unsafe { hegel_c::hegel_run_result_free(ctx, self.raw) });
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

fn map_mode(mode: Mode) -> u32 {
    match mode {
        Mode::TestRun => hegel_c::hegel_mode_t::HEGEL_MODE_TEST_RUN as u32,
        Mode::SingleTestCase => hegel_c::hegel_mode_t::HEGEL_MODE_SINGLE_TEST_CASE as u32,
    }
}

fn map_verbosity(v: Verbosity) -> u32 {
    match v {
        Verbosity::Quiet => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_QUIET as u32,
        Verbosity::Normal => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_NORMAL as u32,
        Verbosity::Verbose => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_VERBOSE as u32,
        Verbosity::Debug => hegel_c::hegel_verbosity_t::HEGEL_VERBOSITY_DEBUG as u32,
    }
}

fn map_backend(backend: Option<Backend>) -> u32 {
    match backend {
        None => hegel_c::hegel_backend_t::HEGEL_BACKEND_AUTO as u32,
        Some(Backend::Default) => hegel_c::hegel_backend_t::HEGEL_BACKEND_DEFAULT as u32,
        Some(Backend::Urandom) => hegel_c::hegel_backend_t::HEGEL_BACKEND_URANDOM as u32,
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
