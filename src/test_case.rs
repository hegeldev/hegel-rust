use crate::control::{
    AssumeFailed, InternalError, InvalidArgument, LoopDone, StopTest, hegel_internal_assert,
    hegel_internal_error, raise_control,
};
use crate::ffi::CTestCase;
use crate::generators::Generator;
use crate::runner::Mode;
use parking_lot::Mutex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;

#[diagnostic::on_unimplemented(
    message = "The first parameter in a #[composite] generator must have type TestCase.",
    label = "This type does not match `TestCase`."
)]
pub trait __IsTestCase {}
impl __IsTestCase for TestCase {}
pub fn __assert_is_test_case<T: __IsTestCase>() {}

/// Raise an invalid-argument (usage) error carrying `message`.
///
/// The same usage error can be detected either while a test case is running
/// (e.g. an inline `tc.draw(gs::sampled_from(&[]))`, or a bound check inside a
/// schema build) or up front, before any run (constructing a generator and
/// validating its arguments eagerly). To read cleanly in both cases:
///
/// - **Inside a test context**, the error unwinds as a typed
///   [`InvalidArgument`] control payload so the lifecycle aborts the run
///   with the message rather than shrinking it as a counterexample.
/// - **Outside any test run**, there is no lifecycle to catch a payload, so
///   the message is panicked directly.
///
/// Either way the user sees only the bare message. Prefer the
/// [`invalid_argument!`] macro, which formats its arguments.
#[track_caller]
pub(crate) fn raise_invalid_argument(message: std::fmt::Arguments<'_>) -> ! {
    if crate::control::currently_in_test_context() {
        raise_control(InvalidArgument(message.to_string()));
    } else {
        panic!("{message}");
    }
}

/// Raise an invalid-argument (usage) error, formatting like [`format!`].
///
/// Use this for every caller-configuration mistake a generator or
/// `tc.target()` detects, in place of a bare `panic!`. See
/// [`raise_invalid_argument`] for how the message is surfaced in and out of a
/// test run.
macro_rules! invalid_argument {
    ($($arg:tt)*) => {
        $crate::test_case::raise_invalid_argument(::std::format_args!($($arg)*))
    };
}
pub(crate) use invalid_argument;

/// Translate a non-`HEGEL_OK` libhegel result code into the matching
/// control-flow unwind. Mirrors the previous `DataSourceError` mapping, but
/// over the C ABI's `hegel_result_t` codes:
///
/// - `HEGEL_E_STOP_TEST` — the engine ran out of data for this case.
/// - `HEGEL_E_ASSUME` — the engine rejected the draw (an assumption failed).
/// - `HEGEL_E_INVALID_ARG` — a caller-supplied argument (typically a
///   generator's schema) was semantically invalid; the diagnostic is read
///   synchronously from this thread's libhegel error context.
/// - `HEGEL_E_ALREADY_COMPLETE` — the test case has finished. Unreachable
///   from a test body (the outcome is reported only after the body returns),
///   so it means a `TestCase` outlived its test — typically moved to a thread
///   that was never joined — and the panic message says so.
/// - anything else — an engine/framework invariant we don't expect on the hot
///   path; treat it as an internal error rather than a shrinkable failure.
///   This includes `HEGEL_E_CONCURRENT_USE`: the frontend never drives one
///   handle from two threads (`clone` forks a fresh handle, `TestCase` is
///   `!Sync`, and `hegel_mark_complete` waits instead of erroring), so it
///   cannot arise here in correct use.
#[track_caller]
pub(crate) fn raise_for_rc(rc: hegel_c::hegel_result_t) -> ! {
    use hegel_c::hegel_result_t::*;
    match rc {
        HEGEL_E_STOP_TEST => raise_control(StopTest),
        HEGEL_E_ASSUME => raise_control(AssumeFailed), // nocov
        HEGEL_E_INVALID_ARG => invalid_argument!("{}", crate::ffi::last_error_string()),
        HEGEL_E_ALREADY_COMPLETE => panic!(
            "this test case has already finished; was the TestCase moved to a \
             thread that outlived the test? Join any thread that draws before \
             the test returns."
        ),
        other => hegel_internal_error!(
            "libhegel returned unexpected code {}: {}",
            other as i32,
            crate::ffi::last_error_string()
        ),
    }
}

pub(crate) struct TestCaseGlobalData {
    mode: Mode,
    /// Whether drawn-value records and notes are surfaced for this test case
    /// (true on the final replay of a failure — unless quiet — or when
    /// verbose output is on).
    /// When false `on_draw` is a no-op, so the draw-recording bookkeeping in
    /// [`TestCase::record_named_draw`] (display-name allocation + `Debug`
    /// rendering of the value) can be skipped entirely.
    emit: bool,
    /// Draw-name bookkeeping shared between every clone of a `TestCase`,
    /// behind a blocking, non-reentrant mutex. The backend handle is no longer
    /// shared here — each `TestCase` instance owns its own libhegel handle (so
    /// clones can be driven concurrently) — so this lock only serialises the
    /// frontend's own draw-name accounting, never backend traffic. No method
    /// holds it while calling back into `TestCase`.
    draw_state: Mutex<DrawState>,
}

pub(crate) struct DrawState {
    named_draw_counts: HashMap<String, usize>,
    named_draw_repeatable: HashMap<String, bool>,
    allocated_display_names: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct TestCaseLocalData {
    span_depth: usize,
    indent: usize,
    on_draw: OutputSink,
}

/// A handle to the current test case.
///
/// This is passed to `#[hegel::test]` functions and provides methods
/// for drawing values, making assumptions, and recording notes.
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let x: i32 = tc.draw(gs::integers());
///     tc.assume(x > 0);
///     tc.note(&format!("x = {}", x));
/// }
/// ```
///
/// # Threading
///
/// `TestCase` is `Send` but not `Sync`. To drive generation from another
/// thread, clone the test case and move the clone. Each clone generates
/// from its own *independent stream* of choices: draws on one clone never
/// perturb the values any other clone (or the original) produces, so
/// several threads can generate concurrently and the test stays fully
/// deterministic — the same seed replays the same values on every stream,
/// failures shrink normally, and the shrunk counterexample replays exactly.
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let tc_worker = tc.clone();
///     let handle = std::thread::spawn(move || {
///         tc_worker.draw(gs::integers::<i32>())
///     });
///     let _b: bool = tc.draw(gs::booleans());
///     let n = handle.join().unwrap();
///     let _ = n;
/// }
/// ```
///
/// ## What is guaranteed
///
/// Each clone owns its own stream, so a clone may be moved to and driven
/// from another thread freely, concurrently with every other clone. A
/// *single* clone may only be driven by one thread at a time — the backend
/// rejects concurrent use of one handle outright — which is why you `clone`
/// to hand work to a thread rather than sharing one `TestCase` across
/// threads (the type is `!Sync`, so the compiler enforces this too).
///
/// The clones share the test case's *outcome*: the whole family passes,
/// fails, or is rejected as one test case, and the choice budget is shared
/// across all streams. Everything else about generation is per-stream.
///
/// ## What is not guaranteed
///
/// Determinism extends exactly as far as your own code's determinism. If
/// threads race on *your* state — for example, which of two clones first
/// consumes a value from a shared queue — Hegel replays each stream's
/// values faithfully, but your test may still behave differently run to
/// run, and such failures may not reproduce or shrink well.
///
/// Variable pools and engine-managed collections are shared across clones
/// (an id from one clone works on any other). Using one such object from
/// two threads *at the same time* makes the affected draws depend on
/// scheduling order, which brings back the same replay caveat.
///
/// ## Panics inside spawned threads
///
/// If a worker thread panics with an assumption failure or a backend
/// `StopTest`, that panic stays inside the thread's `JoinHandle` until
/// the main thread joins it. The main thread is responsible for
/// propagating (or suppressing) the panic — typically by calling
/// `handle.join().unwrap()`, which resumes the panic on the main thread
/// so Hegel's runner can observe it.
pub struct TestCase {
    global: Arc<TestCaseGlobalData>,
    local: RefCell<TestCaseLocalData>,
    /// This instance's libhegel handle, shared through the `Arc` with the
    /// lifecycle that created it and with any [`child`](TestCase::child)
    /// instances, so a `TestCase` that escapes its test (moved to a thread
    /// that is never joined) keeps the handle alive rather than dangling —
    /// its later draws fail cleanly because the case has finished.
    /// [`clone`](TestCase::clone) instead gets a fresh handle
    /// (`hegel_test_case_clone`) onto an independent stream of the same
    /// test case, so two clones can be driven from different threads
    /// concurrently without perturbing each other's values.
    handle: Arc<CTestCase>,
}

impl Clone for TestCase {
    fn clone(&self) -> Self {
        TestCase {
            global: self.global.clone(),
            local: RefCell::new(self.local.borrow().clone()),
            handle: Arc::new(self.handle.clone_handle()),
        }
    }
}

impl std::fmt::Debug for TestCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestCase").finish_non_exhaustive()
    }
}

/// A callback invoked for each line of draw/note output during the final replay.
pub(crate) type OutputSink = Arc<dyn Fn(&str) + Send + Sync>;

thread_local! {
    static OUTPUT_OVERRIDE: RefCell<Option<OutputSink>> = const { RefCell::new(None) };
}

/// Install a custom output sink for the duration of `f`, replacing the usual
/// `eprintln!` behavior of draw and note output. Intended for tests that want
/// to capture what a test case would print.
///
/// While active, notes and draws from the final replay go to `sink` instead of
/// stderr. Non-final test cases still drop their draw/note output as usual.
#[doc(hidden)]
pub fn with_output_override<R>(sink: OutputSink, f: impl FnOnce() -> R) -> R {
    let prev = OUTPUT_OVERRIDE.with(|cell| cell.borrow_mut().replace(sink));
    let result = f();
    OUTPUT_OVERRIDE.with(|cell| *cell.borrow_mut() = prev);
    result
}

/// Return a clone of the currently-installed output sink, if any. Lets the
/// run lifecycle's verbose output (stop-reason lines, per-test-case panic
/// diagnostics) flow through `with_output_override` so tests can capture
/// them in-process without having to spawn a subprocess.
pub(crate) fn current_output_sink() -> Option<OutputSink> {
    OUTPUT_OVERRIDE.with(|cell| cell.borrow().clone())
}

/// Emit a single line of verbose runner output, going through the
/// installed output sink if there is one and otherwise to stderr.
pub(crate) fn emit_verbose_line(msg: &str) {
    if let Some(sink) = current_output_sink() {
        sink(msg);
    } else {
        eprintln!("{}", msg);
    }
}

impl TestCase {
    /// `emit` is decided by the lifecycle (`run_lifecycle::run_test_case`):
    /// true on a non-quiet final replay or in verbose mode, where drawn
    /// values and notes should be surfaced.
    pub(crate) fn new(handle: Arc<CTestCase>, emit: bool, mode: Mode) -> Self {
        let override_sink = current_output_sink();
        let on_draw: OutputSink = match override_sink {
            Some(sink) if emit => sink,
            _ if emit => Arc::new(|msg| eprintln!("{}", msg)),
            _ => Arc::new(|_| {}),
        };
        TestCase {
            global: Arc::new(TestCaseGlobalData {
                mode,
                emit,
                draw_state: Mutex::new(DrawState {
                    named_draw_counts: HashMap::new(),
                    named_draw_repeatable: HashMap::new(),
                    allocated_display_names: HashSet::new(),
                }),
            }),
            local: RefCell::new(TestCaseLocalData {
                span_depth: 0,
                indent: 0,
                on_draw,
            }),
            handle,
        }
    }

    pub(crate) fn mode(&self) -> Mode {
        self.global.mode
    }

    /// Acquire the shared draw-name bookkeeping for the duration of `f`.
    ///
    /// Held briefly around draw-state updates, never around whole user-visible
    /// operations. The mutex is non-reentrant, so `f` must not call any other
    /// method that also acquires it.
    pub(crate) fn with_draw_state<R>(&self, f: impl FnOnce(&mut DrawState) -> R) -> R {
        let mut guard = self.global.draw_state.lock();
        f(&mut guard)
    }

    /// Draw a value from a generator.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators as gs;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let x: i32 = tc.draw(gs::integers());
    ///     let s: String = tc.draw(gs::text());
    /// }
    /// ```
    ///
    /// Note: when run inside a `#[hegel::test]`, `draw()` will typically be
    /// rewritten to `__draw_named()` with an appropriate variable name
    /// in order to give better test output.
    pub fn draw<T: std::fmt::Debug>(&self, generator: impl Generator<T>) -> T {
        self.__draw_named(generator, "draw", true)
    }

    /// Draw a value from a generator with a specific name for output.
    ///
    /// When `repeatable` is true, a counter suffix is appended (e.g. `x_1`, `x_2`).
    /// When `repeatable` is false, reusing the same name panics.
    ///
    /// Using the same name with different values of `repeatable` is an error.
    ///
    /// On the final replay of a failing test case, this prints:
    /// - `let name = value;` (when not repeatable)
    /// - `let name_N = value;` (when repeatable)
    ///
    /// Not intended for direct use. This is the target that `#[hegel::test]` rewrites `draw()`
    /// calls to where appropriate.
    pub fn __draw_named<T: std::fmt::Debug>(
        &self,
        generator: impl Generator<T>,
        name: &str,
        repeatable: bool,
    ) -> T {
        let value = generator.do_draw(self);
        if self.local.borrow().span_depth == 0 {
            self.record_named_draw(&value, name, repeatable);
        }
        value
    }

    /// Draw a value from a generator without recording it in the output.
    ///
    /// Unlike [`draw`](Self::draw), this does not require `T: Debug` and
    /// will not print the value in the failing-test summary.
    pub fn draw_silent<T>(&self, generator: impl Generator<T>) -> T {
        generator.do_draw(self)
    }

    /// Assume a condition is true. If false, reject the current test input.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators as gs;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let age: u32 = tc.draw(gs::integers());
    ///     tc.assume(age >= 18);
    /// }
    /// ```
    pub fn assume(&self, condition: bool) {
        if !condition {
            self.reject();
        }
    }

    /// Reject the current test input unconditionally.
    ///
    /// Equivalent to `assume(false)`, but with a `!` return type so that code
    /// following the call is statically known to be unreachable.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators as gs;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let n: i32 = tc.draw(gs::integers());
    ///     let positive: u32 = match u32::try_from(n) {
    ///         Ok(v) => v,
    ///         Err(_) => tc.reject(),
    ///     };
    ///     let _ = positive;
    /// }
    /// ```
    pub fn reject(&self) -> ! {
        raise_control(AssumeFailed);
    }

    /// Note a message which will be displayed with the reported failing test case.
    ///
    /// At the default verbosity, only prints during the final replay of a
    /// failing test case. At [`Verbose`](crate::Verbosity::Verbose) or
    /// higher, prints on every test case.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators as gs;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let x: i32 = tc.draw(gs::integers());
    ///     tc.note(&format!("Generated x = {}", x));
    /// }
    /// ```
    pub fn note(&self, message: &str) {
        let local = self.local.borrow();
        let indent = local.indent;
        (local.on_draw)(&format!("{:indent$}{}", "", message, indent = indent));
    }

    /// Record a targeting observation to help the engine find extreme inputs.
    ///
    /// Call this inside a test body to guide generation toward inputs that
    /// maximise `score`. Inside a `#[hegel::test]`, `#[hegel::main]`, or
    /// `#[hegel::standalone_function]` body, `tc.target(expr)` is rewritten
    /// to call [`target_labelled`](Self::target_labelled) with the source
    /// text of `expr` as the label, so different targeting expressions are
    /// tracked separately by default. Outside that rewrite, `tc.target(score)`
    /// uses the empty label.
    ///
    /// Has no effect during replays or if the test case has been aborted.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators as gs;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let n: u32 = tc.draw(gs::integers::<u32>());
    ///     tc.target(n as f64);
    /// }
    /// ```
    pub fn target(&self, score: f64) {
        self.target_labelled(score, "");
    }

    /// Record a targeting observation under an explicit label.
    ///
    /// The label distinguishes multiple simultaneous targeting goals.
    /// Use this directly when you want a specific label string;
    /// [`target`](Self::target) is the usual entry point and will be
    /// rewritten to call this with the source expression as the label
    /// inside a `#[hegel::test]` body.
    ///
    /// Has no effect during replays or if the test case has been aborted.
    pub fn target_labelled(&self, score: f64, label: impl Into<String>) {
        let label = label.into();
        let outcome = self.with_ctc(|ctc| ctc.target(score, &label));
        if let Err(rc) = outcome {
            raise_for_rc(rc);
        }
    }

    /// Run `body` in a loop that should runs "logically infinitely" or until
    /// error. Roughly equivalent to a `loop` but with better interaction with
    /// the test runner: This loop will never exit until the test case completes.
    ///
    /// At the start of each iteration a `// Loop iteration N` note is emitted
    /// into the failing-test replay output.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators as gs;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let mut total: i32 = 0;
    ///     tc.repeat(|| {
    ///         let n: i32 = tc.draw(gs::integers().min_value(0).max_value(10));
    ///         total += n;
    ///         assert!(total >= 0);
    ///     });
    /// }
    /// ```
    pub fn repeat<F: FnMut()>(&self, mut body: F) -> ! {
        if self.global.mode == Mode::SingleTestCase {
            self.repeat_single_test_case(&mut body);
        }
        self.repeat_property_test(&mut body);
    }

    fn repeat_single_test_case(&self, body: &mut dyn FnMut()) -> ! {
        let mut iteration: u64 = 0;
        loop {
            iteration += 1;
            self.note(&format!("// Repetition #{}", iteration));

            let prev_indent = self.local.borrow().indent;
            self.local.borrow_mut().indent = prev_indent + 2;
            body();
            self.local.borrow_mut().indent = prev_indent;
        }
    }

    fn repeat_property_test(&self, body: &mut dyn FnMut()) -> ! {
        use crate::generators::{booleans, integers};

        const MAX_SAFE_MIN_SIZE: usize = 1 << 40;
        let min_size = self.draw_silent(integers::<usize>().max_value(MAX_SAFE_MIN_SIZE));

        let mut collection = Collection::new(self, min_size, None);
        let mut iteration: u64 = 0;

        while collection.more() {
            iteration += 1;
            self.note(&format!("// Repetition #{}", iteration));

            let prev_indent = self.local.borrow().indent;
            self.local.borrow_mut().indent = prev_indent + 2;
            let result = catch_unwind(AssertUnwindSafe(&mut *body));
            self.local.borrow_mut().indent = prev_indent;

            match result {
                Ok(()) => {}
                Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => {}
                Err(e)
                    if e.downcast_ref::<StopTest>().is_some()
                        || e.downcast_ref::<InvalidArgument>().is_some()
                        || e.downcast_ref::<InternalError>().is_some() =>
                {
                    resume_unwind(e);
                }
                Err(e) => {
                    self.draw_silent(booleans());
                    resume_unwind(e);
                }
            }
        }

        raise_control(LoopDone);
    }

    pub(crate) fn child(&self, extra_indent: usize) -> Self {
        let local = self.local.borrow();
        TestCase {
            global: self.global.clone(),
            local: RefCell::new(TestCaseLocalData {
                span_depth: 0,
                indent: local.indent + extra_indent,
                on_draw: local.on_draw.clone(),
            }),
            handle: Arc::clone(&self.handle),
        }
    }

    fn record_named_draw<T: std::fmt::Debug>(&self, value: &T, name: &str, repeatable: bool) {
        let emit = self.global.emit;

        let display_name = self.with_draw_state(|draw_state| {
            match draw_state.named_draw_repeatable.get(name) {
                Some(&prev) if prev != repeatable => {
                    hegel_internal_error!(
                        "__draw_named: name {:?} used with inconsistent repeatable flag \
                         (was {}, now {})",
                        name,
                        prev,
                        repeatable
                    );
                }
                Some(_) => {}
                None => {
                    draw_state
                        .named_draw_repeatable
                        .insert(name.to_string(), repeatable);
                }
            }

            let current_count = match draw_state.named_draw_counts.get_mut(name) {
                Some(count) => {
                    *count += 1;
                    *count
                }
                None => {
                    draw_state.named_draw_counts.insert(name.to_string(), 1);
                    1
                }
            };

            if !repeatable && current_count > 1 {
                hegel_internal_error!(
                    "__draw_named: name {:?} used more than once but repeatable is false",
                    name
                );
            }

            if !emit {
                return None;
            }

            let display = if repeatable {
                let mut candidate = current_count;
                loop {
                    let name = format!("{}_{}", name, candidate);
                    if draw_state.allocated_display_names.insert(name.clone()) {
                        break name;
                    }
                    candidate += 1;
                }
            } else {
                let name = name.to_string();
                draw_state.allocated_display_names.insert(name.clone());
                name
            };
            Some(display)
        });

        let Some(display_name) = display_name else {
            return;
        };

        let local = self.local.borrow();
        let indent = local.indent;

        (local.on_draw)(&format!(
            "{:indent$}let {} = {:?};",
            "",
            display_name,
            value,
            indent = indent
        ));
    }

    /// Run `f` with this instance's own libhegel handle.
    ///
    /// Each `TestCase` instance owns its handle, so there is no shared lock to
    /// take here: libhegel serialises a single handle against concurrent use
    /// itself (returning `HEGEL_E_CONCURRENT_USE`), and clones each carry their
    /// own handle and lock.
    pub(crate) fn with_ctc<R>(&self, f: impl FnOnce(&CTestCase) -> R) -> R {
        f(&self.handle)
    }

    #[doc(hidden)]
    pub fn start_span(&self, label: u64) {
        self.local.borrow_mut().span_depth += 1;
        if let Err(rc) = self.with_ctc(|ctc| ctc.start_span(label)) {
            let mut local = self.local.borrow_mut();
            hegel_internal_assert!(local.span_depth > 0);
            local.span_depth -= 1;
            drop(local);
            raise_for_rc(rc);
        }
    }

    #[doc(hidden)]
    pub fn stop_span(&self, discard: bool) {
        {
            let mut local = self.local.borrow_mut();
            hegel_internal_assert!(local.span_depth > 0);
            local.span_depth -= 1;
        }
        if let Err(rc) = self.with_ctc(|ctc| ctc.stop_span(discard)) {
            raise_for_rc(rc);
        }
    }
}

impl TestCase {
    /// Draw an integer in `[min_value, max_value]` (both within `i64`).
    pub(crate) fn generate_integer_i64(&self, min_value: i64, max_value: i64) -> i64 {
        match self.with_ctc(|ctc| ctc.generate_integer(min_value, max_value)) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw an integer with bounds given as two's-complement little-endian
    /// byte encodings, returning the value's encoding sign-extended to 17
    /// bytes.
    pub(crate) fn generate_integer_le17(&self, min_value: &[u8], max_value: &[u8]) -> [u8; 17] {
        match self.with_ctc(|ctc| ctc.generate_integer_big(min_value, max_value)) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a float according to the full libhegel spec.
    #[allow(clippy::too_many_arguments)]
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
    ) -> f64 {
        let outcome = self.with_ctc(|ctc| {
            ctc.generate_float(
                width,
                min_value,
                max_value,
                allow_nan,
                allow_infinity,
                exclude_min,
                exclude_max,
                smallest_nonzero_magnitude,
            )
        });
        match outcome {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a boolean that is `true` with probability `p`.
    pub(crate) fn generate_boolean(&self, p: f64) -> bool {
        match self.with_ctc(|ctc| ctc.generate_boolean(p)) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a byte string with length in `[min_size, max_size]`.
    pub(crate) fn generate_bytes(&self, min_size: usize, max_size: usize) -> Vec<u8> {
        match self.with_ctc(|ctc| ctc.generate_bytes(min_size as u64, max_size as u64)) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a string described by a prebuilt libhegel string generator.
    pub(crate) fn generate_string(&self, generator: &crate::ffi::StringGenerator) -> String {
        match self.with_ctc(|ctc| ctc.generate_string(generator)) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a Gregorian calendar date.
    pub(crate) fn generate_date(&self) -> hegel_c::hegel_date_t {
        match self.with_ctc(|ctc| ctc.generate_date()) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a time of day.
    pub(crate) fn generate_time(&self) -> hegel_c::hegel_time_t {
        match self.with_ctc(|ctc| ctc.generate_time()) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a naive datetime.
    pub(crate) fn generate_datetime(&self) -> hegel_c::hegel_datetime_t {
        match self.with_ctc(|ctc| ctc.generate_datetime()) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw a UUID's 16 big-endian bytes, optionally forcing the version.
    pub(crate) fn generate_uuid(&self, version: Option<u8>) -> [u8; 16] {
        match self.with_ctc(|ctc| ctc.generate_uuid(version)) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw an IPv4 address.
    pub(crate) fn generate_ipv4(&self) -> std::net::Ipv4Addr {
        match self.with_ctc(|ctc| ctc.generate_ipv4()) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }

    /// Draw an IPv6 address.
    pub(crate) fn generate_ipv6(&self) -> std::net::Ipv6Addr {
        match self.with_ctc(|ctc| ctc.generate_ipv6()) {
            Ok(v) => v,
            Err(rc) => raise_for_rc(rc),
        }
    }
}

/// Uses the backend to determine collection sizing.
///
/// The backend-side collection object is created lazily on the first call to
/// [`more()`](Collection::more).
pub struct Collection<'a> {
    tc: &'a TestCase,
    min_size: usize,
    max_size: Option<usize>,
    handle: Option<i64>,
    finished: bool,
}

impl<'a> Collection<'a> {
    /// Create a new backend-managed collection.
    pub fn new(tc: &'a TestCase, min_size: usize, max_size: Option<usize>) -> Self {
        Collection {
            tc,
            min_size,
            max_size,
            handle: None,
            finished: false,
        }
    }

    fn ensure_initialized(&mut self) -> i64 {
        if self.handle.is_none() {
            let result = self.tc.with_ctc(|ctc| {
                ctc.new_collection(self.min_size as u64, self.max_size.map(|m| m as u64))
            });
            let id = match result {
                Ok(id) => id,
                Err(rc) => raise_for_rc(rc), // nocov
            };
            self.handle = Some(id);
        }
        self.handle.unwrap()
    }

    /// Ask the backend whether to produce another element.
    pub fn more(&mut self) -> bool {
        if self.finished {
            return false; // nocov
        }
        let handle = self.ensure_initialized();
        let result = match self.tc.with_ctc(|ctc| ctc.collection_more(handle)) {
            Ok(b) => b,
            Err(rc) => {
                self.finished = true;
                raise_for_rc(rc);
            }
        };
        if !result {
            self.finished = true;
        }
        result
    }

    /// Reject the last element (don't count it towards the size budget).
    pub fn reject(&mut self, why: Option<&str>) {
        // nocov start
        if self.finished {
            return;
        }
        let handle = self.ensure_initialized();
        let _ = self.tc.with_ctc(|ctc| ctc.collection_reject(handle, why));
        // nocov end
    }
}

#[doc(hidden)]
pub mod labels {
    pub const LIST: u64 = 1;
    pub const LIST_ELEMENT: u64 = 2;
    pub const SET: u64 = 3;
    pub const SET_ELEMENT: u64 = 4;
    pub const MAP: u64 = 5;
    pub const MAP_ENTRY: u64 = 6;
    pub const TUPLE: u64 = 7;
    pub const ONE_OF: u64 = 8;
    pub const OPTIONAL: u64 = 9;
    pub const FIXED_DICT: u64 = 10;
    pub const FLAT_MAP: u64 = 11;
    pub const FILTER: u64 = 12;
    pub const MAPPED: u64 = 13;
    pub const SAMPLED_FROM: u64 = 14;
    pub const ENUM_VARIANT: u64 = 15;
    pub const FEATURE_FLAG: u64 = 16;
}

#[cfg(test)]
#[path = "../tests/embedded/test_case_tests.rs"]
mod tests;
