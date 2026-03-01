mod basic;
mod collection;
mod collections;
mod combinators;
mod compose;
mod formats;
mod from_type;
mod numeric;
mod primitives;
#[cfg(feature = "rand")]
mod random;
mod strings;
mod tuples;
mod value;

// public api
pub use basic::BasicGenerator;
pub use collection::Collection;
pub use collections::{hashmaps, hashsets, vecs, HashMapGenerator, arrays, fixed_dicts};
pub use combinators::{one_of, optional, sampled_from, BoxedGenerator};
pub use compose::{fnv1a_hash, ComposedGenerator};
pub use formats::{dates, datetimes, domains, emails, ip_addresses, times, urls};
pub use from_type::{from_type, DefaultGenerator};
pub use numeric::{floats, integers};
pub use primitives::{booleans, just, unit};
#[cfg(feature = "rand")]
#[cfg_attr(docsrs, doc(cfg(feature = "rand")))]
pub use random::{randoms, HegelRandom, RandomsGenerator};
pub use strings::binary;
pub use strings::{from_regex, text};
pub use tuples::{
    tuples10, tuples11, tuples12, tuples2, tuples3, tuples4, tuples5, tuples6, tuples7, tuples8,
    tuples9,
};

pub(crate) use collections::VecGenerator;
pub(crate) use combinators::{Filtered, FlatMapped, Mapped, OptionalGenerator};
pub(crate) use numeric::{FloatGenerator, IntegerGenerator};
pub(crate) use primitives::BoolGenerator;
pub(crate) use strings::TextGenerator;

use ciborium::Value;

use crate::cbor_utils::cbor_map;

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
    pub(super) fn send_request(
        &self,
        command: &str,
        payload: &Value,
    ) -> Result<Value, StopTestError> {
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

// Re-export for macro compatibility ($crate::generators::test_case_data())
#[doc(hidden)]
pub use crate::control::test_case_data;

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
        panic!("Failed to deserialize value: {}\nValue: {:?}", e, raw);
    })
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
    let data = test_case_data().expect("draw() cannot be called outside of a Hegel test.");
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
