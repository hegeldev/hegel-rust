pub use crate::backend::{DataSource, DataSourceError};
use crate::generators::Generator;
use ciborium::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

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

/// Panic with the appropriate sentinel for the given data source error.
fn panic_on_data_source_error(e: DataSourceError) -> ! {
    match e {
        DataSourceError::StopTest => panic!("{}", STOP_TEST_STRING),
        DataSourceError::Assume => panic!("{}", ASSUME_FAIL_STRING), // nocov
        DataSourceError::ServerError(msg) => panic!("{}", msg),
    }
}

pub(crate) struct TestCaseGlobalData {
    data_source: Box<dyn DataSource>,
    is_last_run: bool,
    draw_state: RefCell<DrawState>,
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
    on_draw: Rc<dyn Fn(&str)>,
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
pub struct TestCase {
    global: Rc<TestCaseGlobalData>,
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

impl TestCase {
    pub(crate) fn new(data_source: Box<dyn DataSource>, is_last_run: bool) -> Self {
        let on_draw: Rc<dyn Fn(&str)> = if is_last_run {
            Rc::new(|msg| eprintln!("{}", msg))
        } else {
            Rc::new(|_| {})
        };
        TestCase {
            global: Rc::new(TestCaseGlobalData {
                data_source,
                is_last_run,
                draw_state: RefCell::new(DrawState {
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
        }
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
            panic!("{}", ASSUME_FAIL_STRING);
        }
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
        if self.global.is_last_run {
            let indent = self.local.borrow().indent; // nocov
            eprintln!("{:indent$}{}", "", message, indent = indent); // nocov
        }
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
        let mut draw_state = self.global.draw_state.borrow_mut();

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

        let display_name = if repeatable {
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
        drop(draw_state);

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

    /// Access the data source for this test case.
    pub(crate) fn data_source(&self) -> &dyn DataSource {
        self.global.data_source.as_ref()
    }

    #[doc(hidden)]
    pub fn start_span(&self, label: u64) {
        self.local.borrow_mut().span_depth += 1;
        if let Err(e) = self.data_source().start_span(label) {
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
        let _ = self.data_source().stop_span(discard);
    }
}

/// Send a schema to the backend and return the raw CBOR response.
#[doc(hidden)]
pub fn generate_raw(tc: &TestCase, schema: &Value) -> Value {
    match tc.data_source().generate(schema) {
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
    handle: Option<String>,
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

    fn ensure_initialized(&mut self) -> &str {
        if self.handle.is_none() {
            let name = match self
                .tc
                .data_source()
                .new_collection(self.min_size as u64, self.max_size.map(|m| m as u64))
            {
                Ok(name) => name,
                Err(e) => panic_on_data_source_error(e), // nocov
            };
            self.handle = Some(name);
        }
        self.handle.as_ref().unwrap()
    }

    /// Ask the backend whether to produce another element.
    pub fn more(&mut self) -> bool {
        if self.finished {
            return false; // nocov
        }
        let handle = self.ensure_initialized().to_string();
        let result = match self.tc.data_source().collection_more(&handle) {
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
    // nocov start
    pub fn reject(&mut self, why: Option<&str>) {
        if self.finished {
            return;
        }
        let handle = self.ensure_initialized().to_string();
        let _ = self.tc.data_source().collection_reject(&handle, why);
    }
    // nocov end
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
