mod collections;
mod combinators;
mod default;
mod fixed_dict;
mod formats;
mod macros;
mod numeric;
mod primitives;
mod strings;
mod tuples;

// public api
pub use collections::{hashmaps, hashsets, vecs};
pub use combinators::{one_of, optional, sampled_from, sampled_from_slice, BoxedGenerator};
pub use fixed_dict::fixed_dicts;
pub use formats::{dates, datetimes, domains, emails, ip_addresses, times, urls};
pub use numeric::{floats, integers};
pub use primitives::{booleans, just, just_any, unit};
pub use strings::{from_regex, text};
pub use tuples::{tuples, tuples3};

pub(crate) use collections::VecGenerator;
pub(crate) use combinators::{Filtered, FlatMapped, Mapped, OptionalGenerator};
pub(crate) use numeric::{FloatGenerator, IntegerGenerator};
pub(crate) use primitives::BoolGenerator;
pub(crate) use strings::TextGenerator;

use serde_json::{json, Value};

/// The execution mode for the Hegel SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum HegelMode {
    #[default]
    External,
    Embedded,
}

pub(crate) mod exit_codes {
    #[allow(dead_code)] // Reserved for future use
    pub const TEST_FAILURE: i32 = 1;
    pub const SOCKET_ERROR: i32 = 134;
}
use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader, Write};
use std::marker::PhantomData;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// Mode and State Management (Thread-Local)
// ============================================================================

thread_local! {
    /// Current execution mode
    static MODE: Cell<HegelMode> = const { Cell::new(HegelMode::External) };
    /// Whether this is the last run (for note() output in embedded mode)
    static IS_LAST_RUN: Cell<bool> = const { Cell::new(false) };
}

/// Get the current execution mode.
pub(crate) fn current_mode() -> HegelMode {
    MODE.with(|m| m.get())
}

/// Check if this is the last run.
pub(crate) fn is_last_run() -> bool {
    IS_LAST_RUN.with(|r| r.get())
}

/// Set the current execution mode (used by embedded module).
pub(crate) fn set_mode(mode: HegelMode) {
    MODE.with(|m| m.set(mode));
}

/// Set the is_last_run flag (used by embedded module).
pub(crate) fn set_is_last_run(is_last: bool) {
    IS_LAST_RUN.with(|r| r.set(is_last));
}

/// Print a note message.
///
/// In external mode, this always prints to stderr.
/// In embedded mode, this only prints on the last run.
pub fn note(message: &str) {
    match current_mode() {
        HegelMode::External => eprintln!("{}", message),
        HegelMode::Embedded => {
            if is_last_run() {
                eprintln!("{}", message);
            }
        }
    }
}

// ============================================================================
// Socket Communication with Thread-Local Connection
// ============================================================================

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Thread-local connection state.
/// Connection exists if and only if span_depth > 0.
pub(crate) struct ConnectionState {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
    span_depth: usize,
}

thread_local! {
    static CONNECTION: RefCell<Option<ConnectionState>> = const { RefCell::new(None) };
}

pub(crate) fn is_connected() -> bool {
    CONNECTION.with(|conn| conn.borrow().is_some())
}

pub(crate) fn get_span_depth() -> usize {
    CONNECTION.with(|conn| conn.borrow().as_ref().map(|s| s.span_depth).unwrap_or(0))
}

fn is_debug() -> bool {
    std::env::var("HEGEL_DEBUG").is_ok()
}

fn get_socket_path() -> String {
    std::env::var("HEGEL_SOCKET").expect("HEGEL_SOCKET environment variable not set")
}

/// Open a connection. Panics if already connected.
pub(crate) fn open_connection() {
    CONNECTION.with(|conn| {
        let mut conn = conn.borrow_mut();
        assert!(
            conn.is_none(),
            "open_connection called while already connected"
        );

        let socket_path = get_socket_path();
        let stream = match UnixStream::connect(&socket_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "Failed to connect to Hegel socket at {}: {}",
                    socket_path, e
                );
                std::process::exit(exit_codes::SOCKET_ERROR);
            }
        };

        let writer = stream.try_clone().unwrap_or_else(|e| {
            eprintln!("Failed to clone socket: {}", e);
            std::process::exit(exit_codes::SOCKET_ERROR);
        });
        let reader = BufReader::new(stream);

        *conn = Some(ConnectionState {
            writer,
            reader,
            span_depth: 0,
        });
    });
}

/// Close the connection. Panics if not connected or if spans are still open.
pub(crate) fn close_connection() {
    CONNECTION.with(|conn| {
        let mut conn = conn.borrow_mut();
        let state = conn
            .as_ref()
            .expect("close_connection called while not connected");
        assert_eq!(
            state.span_depth, 0,
            "close_connection called with {} unclosed span(s)",
            state.span_depth
        );
        *conn = None;
    });
}

/// Set the connection from an already-connected stream (used by embedded module).
/// This is used when the SDK creates a server and accepts connections from hegel.
pub(crate) fn set_embedded_connection(stream: UnixStream) {
    CONNECTION.with(|conn| {
        let mut conn = conn.borrow_mut();
        assert!(
            conn.is_none(),
            "set_embedded_connection called while already connected"
        );

        let writer = stream.try_clone().unwrap_or_else(|e| {
            panic!("Failed to clone socket: {}", e);
        });
        let reader = BufReader::new(stream);

        *conn = Some(ConnectionState {
            writer,
            reader,
            span_depth: 0,
        });
    });
}

/// Clear the embedded connection (used by embedded module).
pub(crate) fn clear_embedded_connection() {
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

/// Send a request and receive a response over the thread-local connection.
pub(crate) fn send_request(command: &str, payload: &Value) -> Value {
    let debug = is_debug();
    let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    let request = json!({
        "id": request_id,
        "command": command,
        "payload": payload
    });
    let message = format!("{}\n", request);

    if debug {
        eprint!("REQUEST: {}", message);
    }

    CONNECTION.with(|conn| {
        let mut conn = conn.borrow_mut();
        let state = conn
            .as_mut()
            .expect("send_request called without active connection");

        if let Err(e) = state.writer.write_all(message.as_bytes()) {
            eprintln!("Failed to write to Hegel socket: {}", e);
            std::process::exit(exit_codes::SOCKET_ERROR);
        }

        let mut response = String::new();
        if let Err(e) = state.reader.read_line(&mut response) {
            eprintln!("Failed to read from Hegel socket: {}", e);
            std::process::exit(exit_codes::SOCKET_ERROR);
        }

        if debug {
            eprint!("RESPONSE: {}", response);
        }

        let parsed: Value = match serde_json::from_str(&response) {
            Ok(v) => v,
            Err(e) => {
                panic!(
                    "hegel: failed to parse server response as JSON: {}\nResponse: {}",
                    e, response
                );
            }
        };

        // Verify request ID matches
        let response_id = parsed.get("id").and_then(|v| v.as_u64());
        crate::assume(response_id == Some(request_id));
        crate::assume(parsed.get("error").is_none());

        parsed.get("result").cloned().unwrap_or(Value::Null)
    })
}

pub(crate) fn request_from_schema(schema: &Value) -> Value {
    send_request("generate", schema)
}

/// Generate a value from a schema.
/// If inside a span, uses the existing connection.
/// If not inside a span, opens a connection for this single request (external mode only).
pub fn generate_from_schema<T: serde::de::DeserializeOwned>(schema: &Value) -> T {
    // In embedded mode, connection is already set - don't try to open/close
    let need_connection = !is_connected() && current_mode() == HegelMode::External;
    if need_connection {
        open_connection();
    }

    let result = request_from_schema(schema);

    if need_connection {
        close_connection();
    }

    // Auto-log generated value during final replay (counterexample)
    if is_last_run() {
        eprintln!("Generated: {}", result);
    }

    serde_json::from_value(result.clone()).unwrap_or_else(|e| {
        panic!(
            "hegel: failed to deserialize server response: {}\nValue: {}",
            e, result
        );
    })
}

/// Start a span for grouping related generation.
///
/// Opens a connection if this is the first span (external mode only).
/// Spans help Hypothesis understand the structure of generated data,
/// which improves shrinking. Call `stop_span()` when done.
pub fn start_span(label: u64) {
    // In embedded mode, connection is already set - don't try to open
    if !is_connected() && current_mode() == HegelMode::External {
        open_connection();
    }
    increment_span_depth();
    send_request("start_span", &json!({"label": label}));
}

/// Stop the current span.
///
/// Closes the connection if this is the last span (in external mode only).
/// If `discard` is true, tells Hypothesis this span's data should be discarded
/// (e.g., because a filter rejected it).
pub fn stop_span(discard: bool) {
    decrement_span_depth();
    send_request("stop_span", &json!({"discard": discard}));
    // Only close connection in external mode - in embedded mode, the
    // connection is managed by the embedded module
    if get_span_depth() == 0 && current_mode() == HegelMode::External {
        close_connection();
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
    ///
    /// If `max_attempts` consecutive values fail the predicate, calls `assume(false)`.
    fn filter<F>(self, predicate: F, max_attempts: usize) -> Filtered<T, F, Self>
    where
        Self: Sized,
        F: Fn(&T) -> bool + Send + Sync,
    {
        Filtered {
            source: self,
            predicate,
            max_attempts,
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
