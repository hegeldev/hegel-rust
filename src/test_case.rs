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
/// (e.g. an inline `tc.draw(gs::sampled_from(&[]))`, or a bound check inside
/// a draw) or up front, before any run (constructing a generator and
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
///   generator argument) was semantically invalid; the diagnostic is read
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
    /// Whether the run was declared nondeterministic
    /// ([`Settings::nondeterministic`](crate::Settings::nondeterministic)).
    /// Read by `stateful::run_concurrent` to reject use inside a run that
    /// hasn't declared it.
    nondeterministic: bool,
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

/// A callback invoked for each line of run output — engine progress lines,
/// draw/note output, verbose diagnostics, and the final failure report.
pub(crate) type OutputSink = Arc<dyn Fn(&str) + Send + Sync>;

thread_local! {
    static OUTPUT_OVERRIDE: RefCell<Option<OutputSink>> = const { RefCell::new(None) };
}

/// Install a custom output sink for the duration of `f`, replacing stderr as
/// the destination for all of Hegel's run output. Intended for tests that
/// want to capture what a test run would print.
///
/// A run started while the override is active resolves it once, at start,
/// and routes everything through it for the run's lifetime: the engine's own
/// progress lines (emitted while the engine runs between test cases), draw
/// and note output —
/// including from clones driven on other threads — verbose per-case
/// diagnostics and stop reasons, and the final failure report with its
/// reproducer line. Which of those exist at all is still governed by
/// [`Verbosity`](crate::Verbosity); the override only changes where they go.
#[doc(hidden)]
pub fn with_output_override<R>(sink: OutputSink, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<OutputSink>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OUTPUT_OVERRIDE.with(|cell| *cell.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(OUTPUT_OVERRIDE.with(|cell| cell.borrow_mut().replace(sink)));
    f()
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
///
/// Resolves the sink thread-locally at emit time, so it is only correct for
/// output produced synchronously on the thread that installed the override
/// (e.g. an explicit test case). A run resolves its destination once up
/// front instead — see [`RunOutput`].
pub(crate) fn emit_verbose_line(msg: &str) {
    if let Some(sink) = current_output_sink() {
        sink(msg);
    } else {
        eprintln!("{}", msg);
    }
}

/// The output destination a run resolved when it started.
///
/// Resolved exactly once, on the thread that starts the run, from the
/// installed override ([`with_output_override`]) — and then carried by the
/// run itself. Everything the run emits later flows through this value,
/// wherever it happens: clones driven on other threads (and a run pulled
/// from a different thread than the one that started it) never see the
/// starting thread's thread-local override, so resolving lazily at emit
/// time would send their output to the wrong place.
#[derive(Clone)]
pub(crate) struct RunOutput {
    sink: Option<OutputSink>,
}

impl RunOutput {
    /// Resolve the destination for a run starting now on this thread: the
    /// installed override if there is one, stderr otherwise.
    pub(crate) fn resolve() -> Self {
        RunOutput {
            sink: current_output_sink(),
        }
    }

    /// The resolved sink, for handing to the engine and to test cases;
    /// `None` means stderr.
    pub(crate) fn sink(&self) -> Option<&OutputSink> {
        self.sink.as_ref()
    }

    /// Emit one line of output (no trailing newline).
    pub(crate) fn line(&self, msg: &str) {
        match &self.sink {
            Some(sink) => sink(msg),
            None => eprintln!("{msg}"),
        }
    }

    /// Emit a pre-rendered, newline-terminated block exactly as it would
    /// appear on stderr; the sink receives it as individual lines.
    pub(crate) fn block(&self, text: &str) {
        match &self.sink {
            Some(sink) => {
                for line in text.trim_end_matches('\n').split('\n') {
                    sink(line);
                }
            }
            None => eprint!("{text}"),
        }
    }
}

impl TestCase {
    /// `emit` is decided by the lifecycle (`run_lifecycle::run_test_case`):
    /// true on a non-quiet final replay or in verbose mode, where drawn
    /// values and notes should be surfaced. `sink` is the run's resolved
    /// output destination ([`RunOutput::sink`]) — passed in rather than read
    /// from the thread-local override so that a test case created here and
    /// then driven from another thread still prints to the right place.
    pub(crate) fn new(
        handle: Arc<CTestCase>,
        emit: bool,
        mode: Mode,
        nondeterministic: bool,
        sink: Option<OutputSink>,
    ) -> Self {
        let on_draw: OutputSink = if emit {
            let raw: OutputSink = sink.unwrap_or_else(|| Arc::new(|msg| eprintln!("{}", msg)));
            Arc::new(move |msg| match crate::stateful::current_worker_index() {
                Some(worker) => raw(&format!("[worker {worker}] {msg}")),
                None => raw(msg),
            })
        } else {
            Arc::new(|_| {})
        };
        TestCase {
            global: Arc::new(TestCaseGlobalData {
                mode,
                emit,
                nondeterministic,
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

    /// Whether the run was declared nondeterministic
    /// ([`Settings::nondeterministic`](crate::Settings::nondeterministic)).
    pub(crate) fn nondeterministic(&self) -> bool {
        self.global.nondeterministic
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
            let result = catch_unwind(AssertUnwindSafe(&mut *body));
            self.local.borrow_mut().indent = prev_indent;

            match result {
                Ok(()) => {}
                Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => {}
                Err(e) => resume_unwind(e),
            }
        }
    }

    fn repeat_property_test(&self, body: &mut dyn FnMut()) -> ! {
        use crate::generators::{booleans, integers};

        let max_safe_min_size = usize::try_from(1u64 << 40).unwrap_or(usize::MAX / 2);
        let min_size = self.draw_silent(integers::<usize>().max_value(max_safe_min_size));

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
    /// Run a draw against this instance's libhegel handle, raising the
    /// appropriate control-flow payload on failure.
    fn draw_or_raise<T>(
        &self,
        f: impl FnOnce(&CTestCase) -> Result<T, hegel_c::hegel_result_t>,
    ) -> T {
        self.with_ctc(f).unwrap_or_else(|rc| raise_for_rc(rc))
    }

    /// Draw an integer in `[min_value, max_value]` (both within `i64`).
    pub(crate) fn generate_integer_i64(&self, min_value: i64, max_value: i64) -> i64 {
        self.draw_or_raise(|ctc| ctc.generate_integer(min_value, max_value))
    }

    /// Draw an integer with bounds given as two's-complement little-endian
    /// byte encodings, returning the value's encoding sign-extended to 17
    /// bytes.
    pub(crate) fn generate_integer_le17(&self, min_value: &[u8], max_value: &[u8]) -> [u8; 17] {
        self.draw_or_raise(|ctc| ctc.generate_integer_big(min_value, max_value))
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
        self.draw_or_raise(|ctc| {
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
        })
    }

    /// Draw a boolean that is `true` with probability `p`.
    pub(crate) fn generate_boolean(&self, p: f64) -> bool {
        self.draw_or_raise(|ctc| ctc.generate_boolean(p))
    }

    /// Draw a byte string with length in `[min_size, max_size]`.
    pub(crate) fn generate_bytes(&self, min_size: usize, max_size: usize) -> Vec<u8> {
        self.draw_or_raise(|ctc| ctc.generate_bytes(min_size as u64, max_size as u64))
    }

    /// Draw a string described by a prebuilt libhegel string generator.
    pub(crate) fn generate_string(&self, generator: &crate::ffi::StringGenerator) -> String {
        self.draw_or_raise(|ctc| ctc.generate_string(generator))
    }

    /// Draw a Gregorian calendar date in `[min, max]`.
    pub(crate) fn generate_date(
        &self,
        min: hegel_c::hegel_date_t,
        max: hegel_c::hegel_date_t,
    ) -> hegel_c::hegel_date_t {
        self.draw_or_raise(|ctc| ctc.generate_date(min, max))
    }

    /// Draw a time of day in `[min, max]`.
    pub(crate) fn generate_time(
        &self,
        min: hegel_c::hegel_time_t,
        max: hegel_c::hegel_time_t,
    ) -> hegel_c::hegel_time_t {
        self.draw_or_raise(|ctc| ctc.generate_time(min, max))
    }

    /// Draw a naive datetime in `[min, max]`.
    pub(crate) fn generate_datetime(
        &self,
        min: hegel_c::hegel_datetime_t,
        max: hegel_c::hegel_datetime_t,
    ) -> hegel_c::hegel_datetime_t {
        self.draw_or_raise(|ctc| ctc.generate_datetime(min, max))
    }

    /// Draw a UUID's 16 big-endian bytes, optionally forcing the version.
    pub(crate) fn generate_uuid(&self, version: Option<u8>) -> [u8; 16] {
        self.draw_or_raise(|ctc| ctc.generate_uuid(version))
    }

    /// Draw an IPv4 address.
    pub(crate) fn generate_ipv4(&self) -> std::net::Ipv4Addr {
        self.draw_or_raise(|ctc| ctc.generate_ipv4())
    }

    /// Draw an IPv6 address.
    pub(crate) fn generate_ipv6(&self) -> std::net::Ipv6Addr {
        self.draw_or_raise(|ctc| ctc.generate_ipv6())
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
            return false;
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
        if self.finished {
            return;
        }
        let handle = self.ensure_initialized();
        let _ = self.tc.with_ctc(|ctc| ctc.collection_reject(handle, why));
    }
}

#[doc(hidden)]
pub mod labels {
    use hegel_c::hegel_label_t;

    pub const LIST: u64 = hegel_label_t::HEGEL_LABEL_LIST as u64;
    pub const LIST_ELEMENT: u64 = hegel_label_t::HEGEL_LABEL_LIST_ELEMENT as u64;
    pub const SET: u64 = hegel_label_t::HEGEL_LABEL_SET as u64;
    pub const SET_ELEMENT: u64 = hegel_label_t::HEGEL_LABEL_SET_ELEMENT as u64;
    pub const MAP: u64 = hegel_label_t::HEGEL_LABEL_MAP as u64;
    pub const MAP_ENTRY: u64 = hegel_label_t::HEGEL_LABEL_MAP_ENTRY as u64;
    pub const TUPLE: u64 = hegel_label_t::HEGEL_LABEL_TUPLE as u64;
    pub const ONE_OF: u64 = hegel_label_t::HEGEL_LABEL_ONE_OF as u64;
    pub const OPTIONAL: u64 = hegel_label_t::HEGEL_LABEL_OPTIONAL as u64;
    pub const FIXED_DICT: u64 = hegel_label_t::HEGEL_LABEL_FIXED_DICT as u64;
    pub const FLAT_MAP: u64 = hegel_label_t::HEGEL_LABEL_FLAT_MAP as u64;
    pub const FILTER: u64 = hegel_label_t::HEGEL_LABEL_FILTER as u64;
    pub const MAPPED: u64 = hegel_label_t::HEGEL_LABEL_MAPPED as u64;
    pub const SAMPLED_FROM: u64 = hegel_label_t::HEGEL_LABEL_SAMPLED_FROM as u64;
    pub const ENUM_VARIANT: u64 = hegel_label_t::HEGEL_LABEL_ENUM_VARIANT as u64;
    pub const FEATURE_FLAG: u64 = hegel_label_t::HEGEL_LABEL_FEATURE_FLAG as u64;
}

#[cfg(test)]
#[path = "../tests/embedded/test_case_tests.rs"]
mod tests;

/// The conventional full ranges for the structured draws: years 1..=9999
/// (what Hypothesis's `dates()` spans) and the whole microsecond-resolution
/// day.
pub(crate) mod full_ranges {
    pub(crate) const MIN_DATE: hegel_c::hegel_date_t = hegel_c::hegel_date_t {
        year: 1,
        month: 1,
        day: 1,
    };
    pub(crate) const MAX_DATE: hegel_c::hegel_date_t = hegel_c::hegel_date_t {
        year: 9999,
        month: 12,
        day: 31,
    };
    pub(crate) const MIDNIGHT: hegel_c::hegel_time_t = hegel_c::hegel_time_t {
        hour: 0,
        minute: 0,
        second: 0,
        microsecond: 0,
    };
    pub(crate) const LAST_MICROSECOND: hegel_c::hegel_time_t = hegel_c::hegel_time_t {
        hour: 23,
        minute: 59,
        second: 59,
        microsecond: 999_999,
    };
    pub(crate) const MIN_DATETIME: hegel_c::hegel_datetime_t = hegel_c::hegel_datetime_t {
        date: MIN_DATE,
        time: MIDNIGHT,
    };
    pub(crate) const MAX_DATETIME: hegel_c::hegel_datetime_t = hegel_c::hegel_datetime_t {
        date: MAX_DATE,
        time: LAST_MICROSECOND,
    };
}
