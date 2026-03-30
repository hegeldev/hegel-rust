use crate::cbor_utils::{cbor_map, map_insert};
use crate::generators::Generator;
use crate::protocol::{Channel, Connection, SERVER_CRASHED_MESSAGE};
use crate::runner::Verbosity;
use ciborium::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

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

/// Error indicating the server ran out of data for this test case.
#[derive(Debug)]
pub struct StopTestError;
impl std::fmt::Display for StopTestError {
    // nocov start
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // nocov end
        write!(f, "Server ran out of data (StopTest)") // nocov
    }
}
impl std::error::Error for StopTestError {}

static PROTOCOL_DEBUG: LazyLock<bool> = LazyLock::new(|| {
    // nocov start
    matches!(
        // nocov end
        std::env::var("HEGEL_PROTOCOL_DEBUG")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "1" | "true"
    )
});

pub(crate) const ASSUME_FAIL_STRING: &str = "__HEGEL_ASSUME_FAIL";

/// The sentinel string used to identify overflow/StopTest panics.
/// Distinct from ASSUME_FAIL_STRING so callers can tell user-initiated
/// assumption failures apart from server-initiated data exhaustion.
pub(crate) const STOP_TEST_STRING: &str = "__HEGEL_STOP_TEST";

pub(crate) struct TestCaseGlobalData {
    #[allow(dead_code)]
    connection: Arc<Connection>,
    channel: Channel,
    verbosity: Verbosity,
    is_last_run: bool,
    test_aborted: bool,
}

#[derive(Clone)]
pub(crate) struct TestCaseLocalData {
    span_depth: usize,
    draw_count: usize,
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
    global: Rc<RefCell<TestCaseGlobalData>>,
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
    pub(crate) fn new(
        connection: Arc<Connection>,
        channel: Channel,
        verbosity: Verbosity,
        is_last_run: bool,
    ) -> Self {
        let on_draw: Rc<dyn Fn(&str)> = if is_last_run {
            Rc::new(|msg| eprintln!("{}", msg))
        } else {
            Rc::new(|_| {})
        };
        TestCase {
            global: Rc::new(RefCell::new(TestCaseGlobalData {
                connection,
                channel,
                verbosity,
                is_last_run,
                test_aborted: false,
            })),
            local: RefCell::new(TestCaseLocalData {
                span_depth: 0,
                draw_count: 0,
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
    pub fn draw<T: std::fmt::Debug>(&self, generator: impl Generator<T>) -> T {
        let value = generator.do_draw(self);
        if self.local.borrow().span_depth == 0 {
            self.record_draw(&value);
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
        if self.global.borrow().is_last_run {
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
                draw_count: 0,
                indent: local.indent + extra_indent,
                on_draw: local.on_draw.clone(),
            }),
        }
    }

    fn record_draw<T: std::fmt::Debug>(&self, value: &T) {
        let mut local = self.local.borrow_mut();
        local.draw_count += 1;
        let count = local.draw_count;
        let indent = local.indent;
        (local.on_draw)(&format!(
            "{:indent$}Draw {}: {:?}",
            "",
            count,
            value,
            indent = indent
        ));
    }

    #[doc(hidden)]
    pub fn start_span(&self, label: u64) {
        self.local.borrow_mut().span_depth += 1;
        if let Err(StopTestError) = self.send_request("start_span", &cbor_map! {"label" => label}) {
            // nocov start
            let mut local = self.local.borrow_mut();
            assert!(local.span_depth > 0);
            local.span_depth -= 1;
            drop(local);
            // nocov end
            panic!("{}", STOP_TEST_STRING); // nocov
        }
    }

    #[doc(hidden)]
    pub fn stop_span(&self, discard: bool) {
        {
            let mut local = self.local.borrow_mut();
            assert!(local.span_depth > 0);
            local.span_depth -= 1;
        }
        let _ = self.send_request("stop_span", &cbor_map! {"discard" => discard});
    }

    /// Returns Err(StopTestError) if the server sends an overflow error.
    pub(crate) fn send_request(
        &self,
        command: &str,
        payload: &Value,
    ) -> Result<Value, StopTestError> {
        let mut global = self.global.borrow_mut();

        // If a previous request already triggered overflow/StopTest, the server
        // has closed this channel. Don't send another request—it would block.
        // (The channel-level closed check is also enforced, but this gives a
        // clean StopTestError instead of an io::Error.)
        if global.test_aborted {
            return Err(StopTestError); // nocov
        }
        let debug = *PROTOCOL_DEBUG || global.verbosity == Verbosity::Debug;

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
            eprintln!("REQUEST: {:?}", request); // nocov
        }

        let result = global.channel.request_cbor(&request);
        drop(global);

        match result {
            Ok(response) => {
                if debug {
                    eprintln!("RESPONSE: {:?}", response); // nocov
                }
                Ok(response)
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("overflow")
                    || error_msg.contains("StopTest")
                    || error_msg.contains("channel is closed")
                {
                    if debug {
                        eprintln!("RESPONSE: StopTest/overflow"); // nocov
                    }
                    let mut global = self.global.borrow_mut();
                    global.channel.mark_closed();
                    global.test_aborted = true;
                    drop(global);
                    Err(StopTestError)
                } else if error_msg.contains("FlakyStrategyDefinition")
                    // nocov start
                    || error_msg.contains("FlakyReplay")
                // nocov end
                {
                    // Abort the test case; the server will report the flaky
                    // error in the test_done results, which runner.rs handles.
                    let mut global = self.global.borrow_mut();
                    global.channel.mark_closed();
                    global.test_aborted = true;
                    drop(global);
                    Err(StopTestError)
                // nocov start
                } else if self.global.borrow().connection.server_has_exited() {
                    // nocov end
                    panic!("{}", SERVER_CRASHED_MESSAGE); // nocov
                } else {
                    panic!("Failed to communicate with Hegel: {}", e); // nocov
                }
            }
        }
    }

    // --- Methods for runner access ---

    pub(crate) fn test_aborted(&self) -> bool {
        self.global.borrow().test_aborted
    }

    pub(crate) fn send_mark_complete(&self, mark_complete: &Value) {
        let mut global = self.global.borrow_mut();
        let _ = global.channel.request_cbor(mark_complete);
        let _ = global.channel.close();
    }
}

/// Send a schema to the server and return the raw CBOR response.
#[doc(hidden)]
pub fn generate_raw(tc: &TestCase, schema: &Value) -> Value {
    match tc.send_request("generate", &cbor_map! {"schema" => schema.clone()}) {
        Ok(v) => v,
        Err(StopTestError) => {
            panic!("{}", STOP_TEST_STRING);
        }
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
    /// Create a new server-managed collection.
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
                map_insert(&mut payload, "max_size", max as u64); // nocov
            }
            let response = match self.tc.send_request("new_collection", &payload) {
                Ok(v) => v,
                Err(StopTestError) => {
                    panic!("{}", STOP_TEST_STRING); // nocov
                }
            };
            let name = match response {
                Value::Text(s) => s,
                // nocov start
                _ => panic!(
                    "Expected text response from new_collection, got {:?}",
                    response
                ),
                // nocov end
            };
            self.server_name = Some(name);
        }
        self.server_name.as_ref().unwrap()
    }

    /// Ask the server whether to produce another element.
    pub fn more(&mut self) -> bool {
        if self.finished {
            return false; // nocov
        }
        let server_name = self.ensure_initialized().to_string();
        let response = match self.tc.send_request(
            "collection_more",
            &cbor_map! { "collection" => server_name.as_str() },
        ) {
            Ok(v) => v,
            Err(StopTestError) => {
                self.finished = true;
                panic!("{}", STOP_TEST_STRING);
            }
        };
        let result = match response {
            Value::Bool(b) => b,
            _ => panic!("Expected bool from collection_more, got {:?}", response), // nocov
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
            // nocov end
            // nocov start
            return;
        }
        let server_name = self.ensure_initialized().to_string();
        let mut payload = cbor_map! {
            "collection" => server_name.as_str()
            // nocov end
        };
        // nocov start
        if let Some(reason) = why {
            // nocov end
            map_insert(&mut payload, "why", reason.to_string()); // nocov
        }
        let _ = self.tc.send_request("collection_reject", &payload); // nocov
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
