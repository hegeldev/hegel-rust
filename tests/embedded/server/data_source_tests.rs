use super::*;
use std::os::unix::net::UnixStream;

#[test]
fn test_protocol_debug_true_when_env_set() {
    // Set the env var BEFORE the LazyLock is first accessed in this binary.
    // No other test in the lib binary touches PROTOCOL_DEBUG, so this is the
    // first access and the closure evaluates with the env var present.
    // This exercises the "1" | "true" arm of the matches! macro.
    unsafe {
        std::env::set_var("HEGEL_PROTOCOL_DEBUG", "true");
    }
    assert!(*PROTOCOL_DEBUG);
    unsafe {
        std::env::remove_var("HEGEL_PROTOCOL_DEBUG");
    }
}

/// Build a `ServerDataSource` whose underlying connection has no live peer.
/// The reader thread exits immediately (empty()), and the write peer is
/// dropped, so any actual `send_request` will fail with `BrokenPipe` —
/// exactly the situation we want when asserting that *client-side*
/// validation panics *before* attempting any IO.
fn make_dead_data_source() -> ServerDataSource {
    let (_dropped_peer, write_end) = UnixStream::pair().unwrap();
    // Connection::new already returns an Arc<Connection>.
    let conn = Connection::new(Box::new(std::io::empty()), Box::new(write_end));
    let stream = conn.new_stream();
    ServerDataSource::new(conn, stream, Verbosity::Quiet)
}

// ── N8: ServerDataSource::target_observation client-side validation ───────
//
// The audit (item N8) flagged that `ServerDataSource::target_observation`
// forwards directly to the Python server without the same client-side
// validation that `NativeDataSource::target_observation` performs (post-A16):
//   1. score must be finite (NaN / ±inf rejected).
//   2. each label may be observed at most once per test case.
//
// Pre-N8 behaviour: bad input was either silently accepted or surfaced as
// a CBOR-round-trip error ("server rejected ...") rather than a clear
// client-side panic naming the user's call site. The tests below build a
// dead stream (no live peer) so that the validation panic *must* fire
// before the would-be send_request call. Without the validation, the
// `let _ = send_request(...)` swallows the BrokenPipe and the function
// returns normally — no panic.

#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_panics_on_nan() {
    let ds = make_dead_data_source();
    ds.target_observation(f64::NAN, "x");
}

#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_panics_on_pos_infinity() {
    let ds = make_dead_data_source();
    ds.target_observation(f64::INFINITY, "x");
}

#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_panics_on_neg_infinity() {
    let ds = make_dead_data_source();
    ds.target_observation(f64::NEG_INFINITY, "x");
}

#[test]
#[should_panic(expected = "would overwrite previous tc.target")]
fn target_observation_panics_on_duplicate_label() {
    let ds = make_dead_data_source();
    ds.target_observation(1.0, "x");
    ds.target_observation(2.0, "x");
}

#[test]
fn target_observation_allows_distinct_labels() {
    // Distinct labels in the same test case should be accepted (not panic).
    // Note: `send_request` will internally fail with BrokenPipe on this dead
    // stream and is swallowed by `let _ = ...`, but the validation must not
    // *itself* reject distinct labels.
    let ds = make_dead_data_source();
    ds.target_observation(1.0, "a");
    ds.target_observation(2.0, "b");
}
