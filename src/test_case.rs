use crate::cbor_utils::{cbor_map, map_insert};
use crate::generators::Generator;
use crate::protocol::{Channel, Connection};
use crate::runner::Verbosity;
use ciborium::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use crate::generators::value;

// This is used only as a mechanism for printing a nice error message when the user forgets the
// test case parameter.
#[diagnostic::on_unimplemented(
    message = "Functions marked with #[composite] must have a first parameter of type TestCase.",
    label = "This type does not match `TestCase`.",
)]
pub trait __IsTestCase {}

static PROTOCOL_DEBUG: LazyLock<bool> = LazyLock::new(|| {
    matches!(
        std::env::var("HEGEL_PROTOCOL_DEBUG")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "1" | "true"
    )
});

/// The sentinel string used to identify assume-rejection panics.
pub(crate) const ASSUME_FAIL_STRING: &str = "__HEGEL_ASSUME_FAIL";

pub(crate) struct TestCaseData {
    #[allow(dead_code)]
    connection: Arc<Connection>,
    channel: Channel,
    span_depth: usize,
    verbosity: Verbosity,
    is_last_run: bool,
    output: Vec<String>,
    draw_count: usize,
    test_aborted: bool,
}

/// A handle to the current test case.
///
/// This is passed to `#[hegel::test]` functions and provides methods
/// for drawing values, making assumptions, and recording notes.
///
/// # Example
///
/// ```no_run
/// use hegel::generators;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let x: i32 = tc.draw(generators::integers());
///     tc.assume(x > 0);
///     tc.note(&format!("x = {}", x));
/// }
/// ```
#[derive(Clone)]
pub struct TestCase {
    inner: Rc<RefCell<TestCaseData>>,
}

impl std::fmt::Debug for TestCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestCase").finish_non_exhaustive()
    }
}

impl TestCase {
    pub(crate) fn new(
        connection: Arc<Connection>,
        channel: Channel,
        verbosity: Verbosity,
        is_last_run: bool,
    ) -> Self {
        TestCase {
            inner: Rc::new(RefCell::new(TestCaseData {
                connection,
                channel,
                span_depth: 0,
                verbosity,
                is_last_run,
                output: Vec::new(),
                draw_count: 0,
                test_aborted: false,
            })),
        }
    }

    /// Draw a value from a generator.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let x: i32 = tc.draw(generators::integers());
    ///     let s: String = tc.draw(generators::text());
    /// }
    /// ```
    pub fn draw<T: std::fmt::Debug>(&self, generator: impl Generator<T>) -> T {
        let value = generator.do_draw(self);
        if self.inner.borrow().span_depth == 0 {
            self.record_draw(&value);
        }
        value
    }

    /// Draw a value from a generator without recording it in the output.
    ///
    /// Unlike [`draw`](Self::draw), this does not require `T: Debug` and
    /// will never print the value in the failing-test summary.
    pub fn draw_silent<T>(&self, generator: impl Generator<T>) -> T {
        generator.do_draw(self)
    }

    /// Assume a condition is true. If false, reject the current test input.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let age: u32 = tc.draw(generators::integers());
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
    /// use hegel::generators;
    ///
    /// #[hegel::test]
    /// fn my_test(tc: hegel::TestCase) {
    ///     let x: i32 = tc.draw(generators::integers());
    ///     tc.note(&format!("Generated x = {}", x));
    /// }
    /// ```
    pub fn note(&self, message: &str) {
        if self.inner.borrow().is_last_run {
            eprintln!("{}", message);
        }
    }

    // --- Implementation details used by generators and macros ---

    fn record_draw<T: std::fmt::Debug>(&self, value: &T) {
        let mut inner = self.inner.borrow_mut();
        if !inner.is_last_run {
            return;
        }
        inner.draw_count += 1;
        let count = inner.draw_count;
        inner.output.push(format!("Draw {}: {:?}", count, value));
    }

    #[doc(hidden)]
    pub fn start_span(&self, label: u64) {
        self.inner.borrow_mut().span_depth += 1;
        if let Err(StopTestError) = self.send_request("start_span", &cbor_map! {"label" => label}) {
            let mut inner = self.inner.borrow_mut();
            assert!(inner.span_depth > 0);
            inner.span_depth -= 1;
            drop(inner);
            self.assume(false);
        }
    }

    #[doc(hidden)]
    pub fn stop_span(&self, discard: bool) {
        {
            let mut inner = self.inner.borrow_mut();
            assert!(inner.span_depth > 0);
            inner.span_depth -= 1;
        }
        let _ = self.send_request("stop_span", &cbor_map! {"discard" => discard});
    }

    /// Returns Err(StopTestError) if the server sends an overflow error.
    pub(crate) fn send_request(
        &self,
        command: &str,
        payload: &Value,
    ) -> Result<Value, StopTestError> {
        let inner = self.inner.borrow();
        let debug = *PROTOCOL_DEBUG || inner.verbosity == Verbosity::Debug;

        let mut entries = vec![(
            Value::Text("command".to_string()),
            Value::Text(command.to_string()),
        )];

        if let Value::Map(map) = payload {
            for (k, v) in map {
                entries.push((k.clone(), v.clone()));
            }
        }

        let request = Value::Map(entries);

        if debug {
            eprintln!("REQUEST: {:?}", request);
        }

        let result = inner.channel.request_cbor(&request);
        drop(inner);

        match result {
            Ok(response) => {
                if debug {
                    eprintln!("RESPONSE: {:?}", response);
                }
                Ok(response)
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("overflow") || error_msg.contains("StopTest") {
                    if debug {
                        eprintln!("RESPONSE: StopTest/overflow");
                    }
                    self.inner.borrow_mut().test_aborted = true;
                    Err(StopTestError)
                } else {
                    panic!("Failed to communicate with Hegel: {}", e);
                }
            }
        }
    }

    // --- Methods for runner access ---

    pub(crate) fn take_output(&self) -> Vec<String> {
        std::mem::take(&mut self.inner.borrow_mut().output)
    }

    pub(crate) fn test_aborted(&self) -> bool {
        self.inner.borrow().test_aborted
    }

    pub(crate) fn send_mark_complete(&self, mark_complete: &Value) {
        let inner = self.inner.borrow();
        let _ = inner.channel.request_cbor(mark_complete);
        let _ = inner.channel.close();
    }
}

impl __IsTestCase for TestCase {}

pub fn __assert_is_test_case<T: __IsTestCase>() {}

/// Send a schema to the server and return the raw CBOR response.
#[doc(hidden)]
pub fn generate_raw(tc: &TestCase, schema: &Value) -> Value {
    match tc.send_request("generate", &cbor_map! {"schema" => schema.clone()}) {
        Ok(v) => v,
        Err(StopTestError) => {
            tc.assume(false);
            unreachable!()
        }
    }
}

#[doc(hidden)]
pub fn generate_from_schema<T: serde::de::DeserializeOwned>(tc: &TestCase, schema: &Value) -> T {
    deserialize_value(generate_raw(tc, schema))
}

/// Custom error for StopTest (overflow) condition.
#[derive(Debug)]
pub struct StopTestError;

impl std::fmt::Display for StopTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Server ran out of data (StopTest)")
    }
}

impl std::error::Error for StopTestError {}

/// Deserialize a raw CBOR value into a Rust type.
///
/// This is a public helper for use by derived generators (proc macros)
/// that need to deserialize individual field values from CBOR.
pub fn deserialize_value<T: serde::de::DeserializeOwned>(raw: Value) -> T {
    let hv = value::HegelValue::from(raw.clone());
    value::from_hegel_value(hv).unwrap_or_else(|e| {
        panic!("Failed to deserialize value: {}\nValue: {:?}", e, raw);
    })
}

/// Uses the hegel server to determine collection sizing.
///
/// The server-side `many` object is created lazily on the first call to
/// [`more()`](Collection::more).
pub struct Collection<'a> {
    tc: &'a TestCase,
    base_name: String,
    min_size: usize,
    max_size: Option<usize>,
    server_name: Option<String>,
    finished: bool,
}

impl<'a> Collection<'a> {
    pub fn new(tc: &'a TestCase, name: &str, min_size: usize, max_size: Option<usize>) -> Self {
        Collection {
            tc,
            base_name: name.to_string(),
            min_size,
            max_size,
            server_name: None,
            finished: false,
        }
    }

    fn ensure_initialized(&mut self) -> &str {
        if self.server_name.is_none() {
            let mut payload = cbor_map! {
                "name" => self.base_name.as_str(),
                "min_size" => self.min_size as u64
            };
            if let Some(max) = self.max_size {
                map_insert(&mut payload, "max_size", max as u64);
            }
            let response = match self.tc.send_request("new_collection", &payload) {
                Ok(v) => v,
                Err(StopTestError) => {
                    self.tc.assume(false);
                    unreachable!()
                }
            };
            let name = match response {
                Value::Text(s) => s,
                _ => panic!(
                    "Expected text response from new_collection, got {:?}",
                    response
                ),
            };
            self.server_name = Some(name);
        }
        self.server_name.as_ref().unwrap()
    }

    pub fn more(&mut self) -> bool {
        if self.finished {
            return false;
        }
        let server_name = self.ensure_initialized().to_string();
        let response = match self.tc.send_request(
            "collection_more",
            &cbor_map! { "collection" => server_name.as_str() },
        ) {
            Ok(v) => v,
            Err(StopTestError) => {
                self.finished = true;
                self.tc.assume(false);
                unreachable!()
            }
        };
        let result = match response {
            Value::Bool(b) => b,
            _ => panic!("Expected bool from collection_more, got {:?}", response),
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
        let server_name = self.ensure_initialized().to_string();
        let mut payload = cbor_map! {
            "collection" => server_name.as_str()
        };
        if let Some(reason) = why {
            map_insert(&mut payload, "why", reason.to_string());
        }
        let _ = self.tc.send_request("collection_reject", &payload);
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
