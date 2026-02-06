mod binary;
mod collections;
mod combinators;
mod compose;
mod default;
mod fixed_dict;
mod formats;
mod macros;
mod numeric;
mod primitives;
#[cfg(feature = "rand")]
mod random;
mod strings;
mod tuples;
mod value;

// public api
pub use binary::binary;
pub use collections::{hashmaps, hashsets, vecs, HashMapGenerator};
pub use combinators::{one_of, optional, sampled_from, sampled_from_slice, BoxedGenerator};
pub use compose::{fnv1a_hash, ComposedGenerator};
pub use default::DefaultGenerator;
pub use fixed_dict::fixed_dicts;
pub use formats::{dates, datetimes, domains, emails, ip_addresses, times, urls};
pub use numeric::{floats, integers};
pub use primitives::{booleans, just, just_any, unit};
#[cfg(feature = "rand")]
#[cfg_attr(docsrs, doc(cfg(feature = "rand")))]
pub use random::{randoms, HegelRandom, RandomsGenerator};
pub use strings::{from_regex, text};
pub use tuples::{tuples, tuples3};

pub(crate) use collections::VecGenerator;
pub(crate) use combinators::{Filtered, FlatMapped, Mapped, OptionalGenerator};
pub(crate) use numeric::{FloatGenerator, IntegerGenerator};
pub(crate) use primitives::BoolGenerator;
pub(crate) use strings::TextGenerator;

use ciborium::Value;

use crate::cbor_helpers::{cbor_map, map_insert};

pub(crate) mod exit_codes {
    pub const SOCKET_ERROR: i32 = 134;
}
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::Arc;

use crate::protocol::{Channel, Connection};

// ============================================================================
// State Management (Thread-Local)
// ============================================================================

thread_local! {
    /// Whether this is the last run (for note() output)
    static IS_LAST_RUN: Cell<bool> = const { Cell::new(false) };
    /// Buffer for generated values during final replay
    static GENERATED_VALUES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Whether the test was aborted due to StopTest (server closed channel)
    pub(crate) static TEST_ABORTED: Cell<bool> = const { Cell::new(false) };
}

/// Check if this is the last run.
pub(crate) fn is_last_run() -> bool {
    IS_LAST_RUN.with(|r| r.get())
}

/// Set the is_last_run flag.
pub(crate) fn set_is_last_run(is_last: bool) {
    IS_LAST_RUN.with(|r| r.set(is_last));
}

/// Buffer a generated value for later output
fn buffer_generated_value(value: &str) {
    GENERATED_VALUES.with(|v| v.borrow_mut().push(value.to_string()));
}

/// Take all buffered generated values, clearing the buffer.
pub(crate) fn take_generated_values() -> Vec<String> {
    GENERATED_VALUES.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Print a note message.
///
/// Only prints on the last run (final replay for counterexample output).
pub fn note(message: &str) {
    if is_last_run() {
        eprintln!("{}", message);
    }
}

// ============================================================================
// Socket Communication with Thread-Local Connection
// ============================================================================

/// Thread-local connection state using the binary protocol.
pub(crate) struct ConnectionState {
    /// Keep the connection alive (actual I/O goes through channel)
    #[allow(dead_code)]
    pub(crate) connection: Arc<Connection>,
    pub(crate) channel: Channel,
    pub(crate) span_depth: usize,
}

thread_local! {
    pub(crate) static CONNECTION: RefCell<Option<ConnectionState>> = const { RefCell::new(None) };
}

fn is_debug() -> bool {
    std::env::var("HEGEL_DEBUG").is_ok()
}

/// Set the connection for the current test case.
/// The channel parameter is the test case channel assigned by the server.
pub(crate) fn set_connection(connection: Arc<Connection>, channel: Channel) {
    CONNECTION.with(|conn| {
        let mut conn = conn.borrow_mut();
        assert!(
            conn.is_none(),
            "set_connection called while already connected"
        );

        *conn = Some(ConnectionState {
            connection,
            channel,
            span_depth: 0,
        });
    });
}

/// Clear the connection after a test case completes.
pub(crate) fn clear_connection() {
    CONNECTION.with(|conn| {
        *conn.borrow_mut() = None;
    });
}

pub(crate) fn increment_span_depth() {
    CONNECTION.with(|conn| {
        let mut conn = conn.borrow_mut();
        let state = conn
            .as_mut()
            .expect("start_span called with no active connection");
        state.span_depth += 1;
    });
}

pub(crate) fn decrement_span_depth() {
    CONNECTION.with(|conn| {
        let mut conn = conn.borrow_mut();
        let state = conn
            .as_mut()
            .expect("stop_span called with no active connection");
        assert!(state.span_depth > 0, "stop_span called with no open spans");
        state.span_depth -= 1;
    });
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

/// Send a request and receive a response over the thread-local connection.
/// Returns Err(StopTestError) if the server sends an overflow error.
pub(crate) fn send_request(command: &str, payload: &Value) -> Result<Value, StopTestError> {
    let debug = is_debug();

    // Build the request message by merging command into the payload map
    let mut entries = vec![(
        Value::Text("command".to_string()),
        Value::Text(command.to_string()),
    )];

    // Merge payload fields into the request
    if let Value::Map(map) = payload {
        for (k, v) in map {
            entries.push((k.clone(), v.clone()));
        }
    }

    let request = Value::Map(entries);

    if debug {
        eprintln!("REQUEST: {:?}", request);
    }

    CONNECTION.with(|conn| {
        let conn = conn.borrow();
        let state = conn
            .as_ref()
            .expect("send_request called without active connection");

        let result = state.channel.request_cbor(&request);

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
                    // Mark test as aborted so the runner skips sending mark_complete
                    // (the server has already moved on from this test case)
                    TEST_ABORTED.with(|aborted| aborted.set(true));
                    Err(StopTestError)
                } else {
                    eprintln!("Failed to communicate with Hegel: {}", e);
                    std::process::exit(exit_codes::SOCKET_ERROR);
                }
            }
        }
    })
}

pub(crate) fn request_from_schema(schema: &Value) -> Result<Value, StopTestError> {
    send_request("generate", &cbor_map! {"schema" => schema.clone()})
}

/// Generate a value from a schema.
pub fn generate_from_schema<T: serde::de::DeserializeOwned>(schema: &Value) -> T {
    let result = match request_from_schema(schema) {
        Ok(v) => v,
        Err(StopTestError) => {
            // Server ran out of data - reject this test case
            crate::assume(false);
            unreachable!("assume(false) should not return")
        }
    };

    if is_last_run() {
        buffer_generated_value(&format!(
            "Generated: {}",
            crate::cbor_helpers::display_value(&result)
        ));
    }

    // Convert to HegelValue — ciborium::Value natively preserves NaN/Infinity
    let hegel_value = value::HegelValue::from(result.clone());
    value::from_hegel_value(hegel_value).unwrap_or_else(|e| {
        panic!(
            "hegel: failed to deserialize server response: {}\nValue: {:?}",
            e, result
        );
    })
}

/// Start a span for grouping related generation.
///
/// Spans help Hypothesis understand the structure of generated data,
/// which improves shrinking. Call `stop_span()` when done.
pub fn start_span(label: u64) {
    increment_span_depth();
    if let Err(StopTestError) = send_request("start_span", &cbor_map! {"label" => label}) {
        decrement_span_depth();
        crate::assume(false);
    }
}

/// Stop the current span.
///
/// If `discard` is true, tells Hypothesis this span's data should be discarded
/// (e.g., because a filter rejected it).
pub fn stop_span(discard: bool) {
    decrement_span_depth();
    // Ignore StopTest errors from stop_span - we're already closing
    let _ = send_request("stop_span", &cbor_map! {"discard" => discard});
}

// ============================================================================
// Server-Managed Collections
// ============================================================================

/// A server-managed collection for controlling element generation.
///
/// Collections use the server's sizing logic (Hypothesis's `many` utility)
/// to determine how many elements to generate, rather than picking a fixed
/// size upfront. This produces better shrinking behavior.
///
/// The server-side `many` object is created lazily on the first call to
/// [`more()`](Collection::more).
///
/// # Example
///
/// ```ignore
/// use hegel::gen::Collection;
///
/// let mut coll = Collection::new("my_list", 0, None);
/// let mut result = Vec::new();
/// while coll.more() {
///     result.push(gen::integers::<i32>().generate());
/// }
/// ```
pub struct Collection {
    base_name: String,
    min_size: usize,
    max_size: Option<usize>,
    server_name: Option<String>,
    finished: bool,
}

impl Collection {
    /// Create a new collection handle.
    ///
    /// The server-side `many` object is not created until the first call
    /// to [`more()`](Collection::more), matching the Python SDK's lazy
    /// initialization behavior.
    pub fn new(name: &str, min_size: usize, max_size: Option<usize>) -> Self {
        Collection {
            base_name: name.to_string(),
            min_size,
            max_size,
            server_name: None,
            finished: false,
        }
    }

    /// Ensure the server-side collection is initialized, returning the server name.
    fn ensure_initialized(&mut self) -> &str {
        if self.server_name.is_none() {
            let mut payload = cbor_map! {
                "name" => self.base_name.as_str(),
                "min_size" => self.min_size as u64
            };
            if let Some(max) = self.max_size {
                map_insert(&mut payload, "max_size", Value::from(max as u64));
            }
            let response = match send_request("new_collection", &payload) {
                Ok(v) => v,
                Err(StopTestError) => {
                    crate::assume(false);
                    unreachable!("assume(false) should not return")
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

    /// Check if more elements should be generated.
    ///
    /// On the first call, this lazily creates the server-side collection.
    /// Returns `false` when the collection has reached its target size.
    pub fn more(&mut self) -> bool {
        if self.finished {
            return false;
        }
        let server_name = self.ensure_initialized().to_string();
        let response = match send_request(
            "collection_more",
            &cbor_map! { "collection" => server_name.as_str() },
        ) {
            Ok(v) => v,
            Err(StopTestError) => {
                self.finished = true;
                crate::assume(false);
                unreachable!("assume(false) should not return")
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
    ///
    /// This is useful for unique collections where a generated element
    /// turned out to be a duplicate.
    pub fn reject(&mut self, why: Option<&str>) {
        if self.finished {
            return;
        }
        let server_name = self.ensure_initialized().to_string();
        let mut payload = cbor_map! {
            "collection" => server_name.as_str()
        };
        if let Some(reason) = why {
            map_insert(&mut payload, "why", Value::Text(reason.to_string()));
        }
        let _ = send_request("collection_reject", &payload);
    }
}

// ============================================================================
// Grouped Generation Helpers
// ============================================================================

/// Run a function within a labeled group.
///
/// Groups related generation calls together, which helps the testing engine
/// understand the structure of generated data and improve shrinking.
///
/// # Example
///
/// ```ignore
/// group(labels::LIST, || {
///     // generate list elements here
/// })
/// ```
pub fn group<T, F: FnOnce() -> T>(label: u64, f: F) -> T {
    start_span(label);
    let result = f();
    stop_span(false);
    result
}

/// Run a function within a labeled group, discarding if the function returns None.
///
/// Useful for filter-like operations where rejected values should be discarded.
pub fn discardable_group<T, F: FnOnce() -> Option<T>>(label: u64, f: F) -> Option<T> {
    start_span(label);
    let result = f();
    stop_span(result.is_none());
    result
}

/// Label constants for spans.
/// These help Hypothesis understand the structure of generated data.
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
    pub const ENUM_VARIANT: u64 = 13;
    pub const SAMPLED_FROM: u64 = 14;
    /// For .map() transformations (distinct from MAP which is for collections)
    pub const MAPPED: u64 = 15;
}

// ============================================================================
// Generate Trait
// ============================================================================

/// The core trait for all generators.
///
/// Generators produce values of type `T` and optionally carry a JSON Schema
/// that describes the values they generate.
pub trait Generate<T>: Send + Sync {
    /// Generate a value.
    fn generate(&self) -> T;

    /// Get the JSON Schema for this generator, if available.
    ///
    /// Schemas enable composition optimizations where a single request to Hegel
    /// can generate complex nested structures.
    fn schema(&self) -> Option<Value>;

    /// Transform generated values using a function.
    ///
    /// The resulting generator has no schema since the transformation
    /// may invalidate the schema's semantics.
    fn map<U, F>(self, f: F) -> Mapped<T, U, F, Self>
    where
        Self: Sized,
        F: Fn(T) -> U + Send + Sync,
    {
        Mapped {
            source: self,
            f,
            _phantom: PhantomData,
        }
    }

    /// Generate a value, then use it to create another generator.
    ///
    /// This is useful for dependent generation where the second value
    /// depends on the first.
    fn flat_map<U, G, F>(self, f: F) -> FlatMapped<T, U, G, F, Self>
    where
        Self: Sized,
        G: Generate<U>,
        F: Fn(T) -> G + Send + Sync,
    {
        FlatMapped {
            source: self,
            f,
            _phantom: PhantomData,
        }
    }

    /// Filter generated values using a predicate.
    fn filter<F>(self, predicate: F) -> Filtered<T, F, Self>
    where
        Self: Sized,
        F: Fn(&T) -> bool + Send + Sync,
    {
        Filtered {
            source: self,
            predicate,
            _phantom: PhantomData,
        }
    }

    /// Convert this generator into a type-erased boxed generator.
    ///
    /// This is useful when you need to store generators of different concrete
    /// types in a collection or struct field.
    ///
    /// The lifetime parameter is inferred from the generator being boxed.
    /// For generators that own all their data, this will be `'static`.
    /// For generators that borrow data, the lifetime will match the borrow.
    fn boxed<'a>(self) -> BoxedGenerator<'a, T>
    where
        Self: Sized + Send + Sync + 'a,
    {
        BoxedGenerator {
            inner: Arc::new(self),
        }
    }
}

// Implement Generate for references to generators
impl<T, G: Generate<T>> Generate<T> for &G {
    fn generate(&self) -> T {
        (*self).generate()
    }

    fn schema(&self) -> Option<Value> {
        (*self).schema()
    }
}
