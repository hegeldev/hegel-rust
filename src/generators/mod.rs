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
pub use self::basic::BasicGenerator;
pub use binary::binary;
pub use collections::{hashmaps, hashsets, vecs, HashMapGenerator};
pub use combinators::{one_of, optional, sampled_from, BoxedGenerator};
pub use compose::{fnv1a_hash, ComposedGenerator};
pub use default::{from_type, DefaultGenerator};
pub use fixed_dict::fixed_dicts;
pub use formats::{dates, datetimes, domains, emails, ip_addresses, times, urls};
pub use numeric::{floats, integers};
pub use primitives::{booleans, just, unit};
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

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::{Arc, LazyLock};

use crate::protocol::{Channel, Connection};
use crate::runner::Verbosity;

static PROTOCOL_DEBUG: LazyLock<bool> = LazyLock::new(|| {
    matches!(
        std::env::var("HEGEL_PROTOCOL_DEBUG")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "1" | "true"
    )
});

// ============================================================================
// TestCaseData — per-test-case state
// ============================================================================

/// Per-test-case state, consolidating all thread-local state into one struct.
///
/// This is an internal implementation detail. Do not use directly.
#[doc(hidden)]
pub struct TestCaseData {
    #[allow(dead_code)]
    connection: Arc<Connection>,
    channel: Channel,
    span_depth: Cell<usize>,
    verbosity: Verbosity,
    is_last_run: bool,
    pub(crate) output: RefCell<Vec<String>>,
    draw_count: Cell<usize>,
    test_aborted: Cell<bool>,
    in_composite: Cell<bool>,
}

impl TestCaseData {
    pub(crate) fn new(
        connection: Arc<Connection>,
        channel: Channel,
        verbosity: Verbosity,
        is_last_run: bool,
    ) -> Self {
        TestCaseData {
            connection,
            channel,
            span_depth: Cell::new(0),
            verbosity,
            is_last_run,
            output: RefCell::new(Vec::new()),
            draw_count: Cell::new(0),
            test_aborted: Cell::new(false),
            in_composite: Cell::new(false),
        }
    }

    pub(crate) fn is_last_run(&self) -> bool {
        self.is_last_run
    }

    pub(crate) fn test_aborted(&self) -> bool {
        self.test_aborted.get()
    }

    pub(crate) fn set_test_aborted(&self, val: bool) {
        self.test_aborted.set(val);
    }

    #[doc(hidden)]
    pub fn in_composite(&self) -> bool {
        self.in_composite.get()
    }

    #[doc(hidden)]
    pub fn set_in_composite(&self, val: bool) {
        self.in_composite.set(val);
    }

    fn increment_span_depth(&self) {
        self.span_depth.set(self.span_depth.get() + 1);
    }

    fn decrement_span_depth(&self) {
        let depth = self.span_depth.get();
        assert!(depth > 0, "stop_span called with no open spans");
        self.span_depth.set(depth - 1);
    }

    pub(crate) fn channel(&self) -> &Channel {
        &self.channel
    }

    fn verbosity(&self) -> Verbosity {
        self.verbosity
    }

    fn start_span(&self, label: u64) {
        self.increment_span_depth();
        if let Err(StopTestError) = self.send_request("start_span", &cbor_map! {"label" => label}) {
            self.decrement_span_depth();
            crate::assume(false);
        }
    }

    fn stop_span(&self, discard: bool) {
        self.decrement_span_depth();
        // Ignore StopTest errors from stop_span - we're already closing
        let _ = self.send_request("stop_span", &cbor_map! {"discard" => discard});
    }

    /// Send a request and receive a response via the channel.
    /// Returns Err(StopTestError) if the server sends an overflow error.
    fn send_request(&self, command: &str, payload: &Value) -> Result<Value, StopTestError> {
        let debug = *PROTOCOL_DEBUG || self.verbosity() == Verbosity::Debug;

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

        let result = self.channel().request_cbor(&request);

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
                    self.set_test_aborted(true);
                    Err(StopTestError)
                } else {
                    panic!("Failed to communicate with Hegel: {}", e);
                }
            }
        }
    }

    /// Send a schema to the server and return the raw CBOR response.
    ///
    /// This is the core generation primitive. It handles StopTest errors
    /// by calling `assume(false)` to mark the test case as invalid.
    pub fn generate_raw(&self, schema: &Value) -> Value {
        match self.send_request("generate", &cbor_map! {"schema" => schema.clone()}) {
            Ok(v) => v,
            Err(StopTestError) => {
                crate::assume(false);
                unreachable!()
            }
        }
    }

    /// Generate a value from a schema, deserializing the result.
    pub fn generate_from_schema<T: serde::de::DeserializeOwned>(&self, schema: &Value) -> T {
        deserialize_value(self.generate_raw(schema))
    }

    /// Run a function within a labeled span group.
    ///
    /// Groups related generation calls together, which helps the testing engine
    /// understand the structure of generated data and improve shrinking.
    pub fn span_group<T, F: FnOnce() -> T>(&self, label: u64, f: F) -> T {
        self.start_span(label);
        let result = f();
        self.stop_span(false);
        result
    }

    /// Run a function within a labeled span group, discarding if the function returns None.
    ///
    /// Useful for filter-like operations where rejected values should be discarded.
    pub fn discardable_span_group<T, F: FnOnce() -> Option<T>>(
        &self,
        label: u64,
        f: F,
    ) -> Option<T> {
        self.start_span(label);
        let result = f();
        self.stop_span(result.is_none());
        result
    }
}

thread_local! {
    pub(crate) static TEST_CASE_DATA: Cell<*const TestCaseData> = const { Cell::new(std::ptr::null()) };
}

/// Get a reference to the current test case's TestCaseData.
///
/// # Panics
/// Panics if called outside of a test case (no active TestCaseData).
#[doc(hidden)]
pub fn test_case_data() -> &'static TestCaseData {
    TEST_CASE_DATA.with(|c| {
        let ptr = c.get();
        assert!(!ptr.is_null(), "no active test case");
        unsafe { &*ptr }
    })
}

/// Print a note message for the final failing test case.
pub fn note(message: &str) {
    let data = test_case_data();
    if data.is_last_run() {
        eprintln!("{}", message);
    }
}

// ============================================================================
// Socket Communication
// ============================================================================

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
        panic!(
            "hegel: failed to deserialize value: {}\nValue: {:?}",
            e, raw
        );
    })
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
/// use hegel::generators::Collection;
///
/// let data = hegel::generators::test_case_data();
/// let mut coll = Collection::new("my_list", 0, None);
/// let mut result = Vec::new();
/// while coll.more(data) {
///     result.push(generators::integers::<i32>().do_draw(data));
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
    fn ensure_initialized(&mut self, data: &TestCaseData) -> &str {
        if self.server_name.is_none() {
            let mut payload = cbor_map! {
                "name" => self.base_name.as_str(),
                "min_size" => self.min_size as u64
            };
            if let Some(max) = self.max_size {
                map_insert(&mut payload, "max_size", Value::from(max as u64));
            }
            let response = match data.send_request("new_collection", &payload) {
                Ok(v) => v,
                Err(StopTestError) => {
                    crate::assume(false);
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

    /// Check if more elements should be generated.
    ///
    /// On the first call, this lazily creates the server-side collection.
    /// Returns `false` when the collection has reached its target size.
    pub fn more(&mut self, data: &TestCaseData) -> bool {
        if self.finished {
            return false;
        }
        let server_name = self.ensure_initialized(data).to_string();
        let response = match data.send_request(
            "collection_more",
            &cbor_map! { "collection" => server_name.as_str() },
        ) {
            Ok(v) => v,
            Err(StopTestError) => {
                self.finished = true;
                crate::assume(false);
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
    ///
    /// This is useful for unique collections where a generated element
    /// turned out to be a duplicate.
    pub fn reject(&mut self, data: &TestCaseData, why: Option<&str>) {
        if self.finished {
            return;
        }
        let server_name = self.ensure_initialized(data).to_string();
        let mut payload = cbor_map! {
            "collection" => server_name.as_str()
        };
        if let Some(reason) = why {
            map_insert(&mut payload, "why", Value::Text(reason.to_string()));
        }
        let _ = data.send_request("collection_reject", &payload);
    }
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
    /// For .map() transformations (distinct from MAP which is for collections)
    pub const MAPPED: u64 = 13;
    pub const SAMPLED_FROM: u64 = 14;
    pub const ENUM_VARIANT: u64 = 15;
}

// ============================================================================
// BasicGenerator
// ============================================================================

/// A basic generator bundles a schema with a parse function.
///
/// This is the key abstraction for schema-based generation. Generators that
/// can be expressed as a single schema implement `as_basic()` to return one.
/// Combinators like `map()` compose BasicGenerators by chaining parse functions
/// while preserving the schema.
pub mod basic {
    use super::TestCaseData;
    use ciborium::Value;
    use std::marker::PhantomData;

    /// A bundled schema + parse function for schema-based generation.
    ///
    /// The lifetime `'a` ties the BasicGenerator to the generator that created it.
    /// `T: 'a` is required because the parse closure returns `T`.
    pub struct BasicGenerator<'a, T> {
        schema: Value,
        parse: Box<dyn Fn(Value) -> T + Send + Sync + 'a>,
        _phantom: PhantomData<fn() -> T>,
    }

    impl<'a, T: 'a> BasicGenerator<'a, T> {
        /// Create a new BasicGenerator from a schema and parse function.
        pub fn new<F: Fn(Value) -> T + Send + Sync + 'a>(schema: Value, f: F) -> Self {
            BasicGenerator {
                schema,
                parse: Box::new(f),
                _phantom: PhantomData,
            }
        }

        /// Get a reference to the schema.
        pub fn schema(&self) -> &Value {
            &self.schema
        }

        /// Parse a raw CBOR value into the generated type.
        pub fn parse_raw(&self, raw: Value) -> T {
            (self.parse)(raw)
        }

        /// Generate a value by sending the schema to the server and parsing the response.
        ///
        /// This is a convenience for `self.parse_raw(data.generate_raw(self.schema()))`.
        pub fn do_draw(&self, data: &TestCaseData) -> T {
            self.parse_raw(data.generate_raw(self.schema()))
        }

        /// Transform the output type by composing a function with the parse.
        ///
        /// The resulting BasicGenerator shares the same schema but applies `f`
        /// after parsing.
        pub fn map<U: 'a, F: Fn(T) -> U + Send + Sync + 'a>(self, f: F) -> BasicGenerator<'a, U> {
            let old_parse = self.parse;
            BasicGenerator {
                schema: self.schema,
                parse: Box::new(move |raw| f(old_parse(raw))),
                _phantom: PhantomData,
            }
        }
    }
}

// ============================================================================
// Generate Trait
// ============================================================================

/// The core trait for all generators.
///
/// Generators produce values of type `T` and optionally provide a
/// [`BasicGenerator`] for server-based generation via `as_basic()`.
pub trait Generate<T>: Send + Sync {
    /// Generate a value. This is an internal method — use [`draw()`] instead.
    fn do_draw(&self, data: &TestCaseData) -> T;

    /// Return a BasicGenerator for schema-based generation, if possible.
    ///
    /// When available, this enables single-request schema-based generation
    /// and allows combinators to compose schemas.
    ///
    /// Returns `None` for generators that cannot be expressed as a schema
    /// (e.g., after `flat_map` or `filter`).
    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        None
    }

    /// Transform generated values using a function.
    ///
    /// If this generator is basic, the resulting generator is also basic
    /// with a composed transform (preserving the schema).
    /// If this generator is not basic, falls back to a MappedGenerator
    /// with span tracking.
    fn map<U, F>(self, f: F) -> Mapped<T, U, F, Self>
    where
        Self: Sized,
        F: Fn(T) -> U + Send + Sync,
    {
        Mapped {
            source: self,
            f: Arc::new(f),
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
    fn do_draw(&self, data: &TestCaseData) -> T {
        (*self).do_draw(data)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        (*self).as_basic()
    }
}

/// Draw a value from a generator, logging it on the final replay.
///
/// This is the primary user-facing API for generating values, analogous
/// to Hypothesis's `data.draw()`. It must not be called inside a
/// `compose!` block — use the `draw` parameter provided by `compose!` instead.
///
/// # Example
///
/// ```no_run
/// use hegel::generators;
///
/// # hegel::hegel(|| {
/// let x: i32 = hegel::draw(&generators::integers::<i32>());
/// let s: String = hegel::draw(&generators::text());
/// # });
/// ```
pub fn draw<T: std::fmt::Debug>(gen: &impl Generate<T>) -> T {
    let data = test_case_data();
    assert!(
        !data.in_composite(),
        "cannot call draw() inside compose!(). Use the draw parameter instead."
    );
    let value = gen.do_draw(data);
    if data.is_last_run() {
        let n = data.draw_count.get() + 1;
        data.draw_count.set(n);
        data.output
            .borrow_mut()
            .push(format!("Draw {}: {:?}", n, value));
    }
    value
}
