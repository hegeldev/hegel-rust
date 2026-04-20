pub use crate::backend::{DataSource, DataSourceError};
use crate::generators::Generator;
use crate::runner::Mode;
use ciborium::Value;
use parking_lot::Mutex;
use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;

use crate::generators::value;

// We use the __IsTestCase trait internally to provide nice error messages for misuses of #[composite].
// It should not be used by users.
//
// The idea is #[composite] calls __assert_is_test_case(<first param>), which errors with our on_unimplemented
// message iff the first param does not have type TestCase.

#[diagnostic::on_unimplemented(
    // NOTE: worth checking if edits to this message should also be applied to the similar-but-different
    // error message in #[composite] in hegel-macros.
    message = "The first parameter in a #[composite] generator must have type TestCase.",
    label = "This type does not match `TestCase`."
)]
pub trait __IsTestCase {}
impl __IsTestCase for TestCase {}
pub fn __assert_is_test_case<T: __IsTestCase>() {}

pub(crate) const ASSUME_FAIL_STRING: &str = "__HEGEL_ASSUME_FAIL";

/// The sentinel string used to identify overflow/StopTest panics.
/// Distinct from ASSUME_FAIL_STRING so callers can tell user-initiated
/// assumption failures apart from backend-initiated data exhaustion.
pub(crate) const STOP_TEST_STRING: &str = "__HEGEL_STOP_TEST";

/// The sentinel string used by `TestCase::repeat` to signal that its loop
/// completed naturally (the collection said "stop" and no panic occurred
/// inside the body). Because `repeat` returns `!`, it has no normal-return
/// path; this panic is how it tells the runner "this test case finished
/// successfully, record it as Valid".
pub(crate) const LOOP_DONE_STRING: &str = "__HEGEL_LOOP_DONE";

/// Panic with the appropriate sentinel for the given data source error.
fn panic_on_data_source_error(e: DataSourceError) -> ! {
    match e {
        DataSourceError::StopTest => panic!("{}", STOP_TEST_STRING),
        DataSourceError::Assume => panic!("{}", ASSUME_FAIL_STRING), // nocov
        DataSourceError::ServerError(msg) => panic!("{}", msg),
    }
}

pub(crate) struct TestCaseGlobalData {
    is_last_run: bool,
    mode: Mode,
    /// Fine-grained lock over the state shared between clones of a
    /// `TestCase`. Acquired briefly around each individual backend call
    /// and around each mutation of the draw-tracking bookkeeping, not
    /// around entire user-visible operations like a `draw`. The mutex is
    /// non-reentrant; no method holds it while calling back into
    /// `TestCase`.
    shared: Mutex<SharedState>,
}

pub(crate) struct SharedState {
    data_source: Box<dyn DataSource>,
    draw_state: DrawState,
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
/// thread, clone the test case and move the clone. Clones share the same
/// underlying backend connection — they are views onto one test case, not
/// independent test cases.
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
///     let n = handle.join().unwrap();
///     let _b: bool = tc.draw(gs::booleans());
///     let _ = n;
/// }
/// ```
///
/// ## What is guaranteed
///
/// Individual backend operations (a single `generate`, `start_span`,
/// `stop_span`, or pool/collection call) are serialised by a shared
/// mutex, so the bytes on the wire to the backend stay well-formed no
/// matter how clones are used across threads.
///
/// This is enough for patterns where threads do not race on generation —
/// for example:
///
/// - Spawn a worker, let it draw, `join` it, then continue on the main
///   thread.
/// - Repeatedly spawn-and-join one worker at a time.
/// - Any pattern where exactly one thread is drawing at a time, with a
///   happens-before relationship (join, channel receive, barrier) between
///   each thread's work.
///
/// ## What is not guaranteed
///
/// Concurrent generation will get progressively better over time, but
/// right now should be considered a borderline-internal feature. If
/// you do not know exactly what you're doing it probably won't work.
///
/// Two or more threads drawing concurrently from clones of the same
/// `TestCase` is allowed by the type system but is **not deterministic**:
/// the order in which draws interleave depends on thread scheduling, and
/// the backend has no way to reproduce that order on replay. Composite
/// draws are also not atomic with respect to other threads — another
/// thread's draws can land between this thread's `start_span` and
/// `stop_span`, corrupting the shrink-friendly span structure. In
/// practice this means such tests may:
///
/// - Produce different values on successive runs of the same seed.
/// - Shrink poorly or not at all.
/// - Surface backend errors (e.g. `StopTest`) in one thread caused by
///   another thread's draws exhausting the budget.
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
    // RefCell makes `TestCase: !Sync`. Local data is per-clone: each clone gets
    // its own span depth, indent, and on_draw. Concurrent use across threads
    // therefore requires cloning, which is enforced by the `!Sync` bound.
    local: RefCell<TestCaseLocalData>,
}

impl Clone for TestCase {
    fn clone(&self) -> Self {
        TestCase {
            global: self.global.clone(),
            local: RefCell::new(self.local.borrow().clone()),
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

fn panic_message(payload: &Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string() // nocov
    }
}

impl TestCase {
    pub(crate) fn new(data_source: Box<dyn DataSource>, is_last_run: bool, mode: Mode) -> Self {
        let override_sink = OUTPUT_OVERRIDE.with(|cell| cell.borrow().clone());
        let on_draw: OutputSink = match override_sink {
            Some(sink) if is_last_run => sink,
            _ if is_last_run => Arc::new(|msg| eprintln!("{}", msg)),
            _ => Arc::new(|_| {}),
        };
        TestCase {
            global: Arc::new(TestCaseGlobalData {
                is_last_run,
                mode,
                shared: Mutex::new(SharedState {
                    data_source,
                    draw_state: DrawState {
                        named_draw_counts: HashMap::new(),
                        named_draw_repeatable: HashMap::new(),
                        allocated_display_names: HashSet::new(),
                    },
                }),
            }),
            local: RefCell::new(TestCaseLocalData {
                span_depth: 0,
                indent: 0,
                on_draw,
            }),
        }
    }

    pub(crate) fn mode(&self) -> Mode {
        self.global.mode
    }

    /// Acquire the shared mutex for the duration of `f`.
    ///
    /// Held briefly around individual backend calls or draw-state updates,
    /// never around whole user-visible operations. The mutex is
    /// non-reentrant, so `f` must not call any other method that also
    /// acquires the shared mutex.
    fn with_shared<R>(&self, f: impl FnOnce(&mut SharedState) -> R) -> R {
        let mut guard = self.global.shared.lock();
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
        panic!("{}", ASSUME_FAIL_STRING);
    }

    /// Note a message which will be displayed with the reported failing test case.
    ///
    /// Only prints during the final replay of a failing test case.
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
        if !self.global.is_last_run {
            return;
        }
        let local = self.local.borrow();
        let indent = local.indent;
        (local.on_draw)(&format!("{:indent$}{}", "", message, indent = indent));
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
                Err(e) => {
                    let msg = panic_message(&e);
                    if msg == ASSUME_FAIL_STRING {
                    } else if msg == STOP_TEST_STRING {
                        resume_unwind(e);
                    } else {
                        self.draw_silent(booleans());
                        resume_unwind(e);
                    }
                }
            }
        }

        panic!("{}", LOOP_DONE_STRING);
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
        }
    }

    fn record_named_draw<T: std::fmt::Debug>(&self, value: &T, name: &str, repeatable: bool) {
        let display_name = self.with_shared(|shared| {
            let draw_state = &mut shared.draw_state;

            match draw_state.named_draw_repeatable.get(name) {
                Some(&prev) if prev != repeatable => {
                    panic!(
                        "__draw_named: name {:?} used with inconsistent repeatable flag (was {}, now {}). \
                        If you have not called __draw_named deliberately yourself, this is likely a bug in \
                        hegel. Please file a bug report at https://github.com/hegeldev/hegel-rust/issues",
                        name, prev, repeatable
                    );
                }
                _ => {
                    draw_state
                        .named_draw_repeatable
                        .insert(name.to_string(), repeatable);
                }
            }

            let count = draw_state
                .named_draw_counts
                .entry(name.to_string())
                .or_insert(0);
            *count += 1;
            let current_count = *count;

            if !repeatable && current_count > 1 {
                panic!(
                    "__draw_named: name {:?} used more than once but repeatable is false. \
                    This is almost certainly a bug in hegel - please report it at https://github.com/hegeldev/hegel-rust/issues",
                    name
                );
            }

            if repeatable {
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
            }
        });

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

    /// Run `f` with access to this test case's data source.
    ///
    /// Acquires the shared mutex for the duration of the call so
    /// concurrent threads don't scramble backend traffic. The closure
    /// must not call back into any other `TestCase` method that would
    /// re-acquire the shared mutex.
    pub(crate) fn with_data_source<R>(&self, f: impl FnOnce(&dyn DataSource) -> R) -> R {
        self.with_shared(|shared| f(shared.data_source.as_ref()))
    }

    /// Report whether the test case has been aborted (StopTest/overflow).
    ///
    /// Used by the runner to decide whether to send `mark_complete`.
    #[cfg(not(feature = "native"))]
    pub(crate) fn test_aborted(&self) -> bool {
        self.with_data_source(|ds| ds.test_aborted())
    }

    /// Send `mark_complete` on this test case's data source.
    #[cfg(not(feature = "native"))]
    pub(crate) fn mark_complete(&self, status: &str, origin: Option<&str>) {
        self.with_data_source(|ds| ds.mark_complete(status, origin));
    }

    #[doc(hidden)]
    pub fn start_span(&self, label: u64) {
        self.local.borrow_mut().span_depth += 1;
        if let Err(e) = self.with_data_source(|ds| ds.start_span(label)) {
            // nocov start
            let mut local = self.local.borrow_mut();
            assert!(local.span_depth > 0);
            local.span_depth -= 1;
            drop(local);
            panic_on_data_source_error(e);
            // nocov end
        }
    }

    #[doc(hidden)]
    pub fn stop_span(&self, discard: bool) {
        {
            let mut local = self.local.borrow_mut();
            assert!(local.span_depth > 0);
            local.span_depth -= 1;
        }
        let _ = self.with_data_source(|ds| ds.stop_span(discard));
    }
}

/// Send a schema to the backend and return the raw CBOR response.
#[doc(hidden)]
pub fn generate_raw(tc: &TestCase, schema: &Value) -> Value {
    match tc.with_data_source(|ds| ds.generate(schema)) {
        Ok(v) => v,
        Err(e) => panic_on_data_source_error(e),
    }
}

#[doc(hidden)]
pub fn generate_from_schema<T: serde::de::DeserializeOwned>(tc: &TestCase, schema: &Value) -> T {
    deserialize_value(generate_raw(tc, schema))
}

/// Deserialize a raw CBOR value into a Rust type.
///
/// This is a public helper for use by derived generators (proc macros)
/// that need to deserialize individual field values from CBOR.
pub fn deserialize_value<T: serde::de::DeserializeOwned>(raw: Value) -> T {
    let hv = value::HegelValue::from(raw.clone());
    value::from_hegel_value(hv).unwrap_or_else(|e| {
        panic!("Failed to deserialize value: {}\nValue: {:?}", e, raw); // nocov
    })
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
            let result = self.tc.with_data_source(|ds| {
                ds.new_collection(self.min_size as u64, self.max_size.map(|m| m as u64))
            });
            let id = match result {
                Ok(id) => id,
                Err(e) => panic_on_data_source_error(e), // nocov
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
        let result = match self.tc.with_data_source(|ds| ds.collection_more(handle)) {
            Ok(b) => b,
            Err(e) => {
                self.finished = true;
                panic_on_data_source_error(e);
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
        let _ = self
            .tc
            .with_data_source(|ds| ds.collection_reject(handle, why));
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
}
