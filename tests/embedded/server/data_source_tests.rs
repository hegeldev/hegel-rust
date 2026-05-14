use super::*;
#[cfg(unix)]
use crate::server::protocol::packet::{Packet, read_packet, write_packet};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

// Mutex so the env-var-mutating tests below serialise with each other —
// modifying process-global env vars from concurrent tests is racy.
static PROTOCOL_DEBUG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Read PROTOCOL_DEBUG from the env via the extracted helper, isolating us
/// from the LazyLock cache (which would only initialise once per binary
/// regardless of subsequent env var changes).
#[test]
fn protocol_debug_from_env_true_for_true_lowercase() {
    let _guard = PROTOCOL_DEBUG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var("HEGEL_PROTOCOL_DEBUG", "true");
    }
    assert!(protocol_debug_from_env());
    unsafe {
        std::env::remove_var("HEGEL_PROTOCOL_DEBUG");
    }
}

#[test]
fn protocol_debug_from_env_true_for_one() {
    let _guard = PROTOCOL_DEBUG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var("HEGEL_PROTOCOL_DEBUG", "1");
    }
    assert!(protocol_debug_from_env());
    unsafe {
        std::env::remove_var("HEGEL_PROTOCOL_DEBUG");
    }
}

#[test]
fn protocol_debug_from_env_false_when_unset() {
    let _guard = PROTOCOL_DEBUG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::remove_var("HEGEL_PROTOCOL_DEBUG");
    }
    assert!(!protocol_debug_from_env());
}

#[test]
fn protocol_debug_from_env_false_for_garbage() {
    let _guard = PROTOCOL_DEBUG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var("HEGEL_PROTOCOL_DEBUG", "yes");
    }
    assert!(!protocol_debug_from_env());
    unsafe {
        std::env::remove_var("HEGEL_PROTOCOL_DEBUG");
    }
}

/// Build a `ServerDataSource` whose underlying connection has no live peer.
/// The reader thread exits immediately (empty()), and the write peer is
/// dropped — `send_request` will detect server-exited and panic. Use this
/// only for tests where validation is expected to fire *before* any IO
/// happens (the finite-score tests below).
#[cfg(unix)]
fn make_dead_data_source() -> ServerDataSource {
    let (_dropped_peer, write_end) = UnixStream::pair().unwrap();
    // Connection::new already returns an Arc<Connection>.
    let conn = Connection::new(Box::new(std::io::empty()), Box::new(write_end));
    let stream = conn.new_stream();
    ServerDataSource::new(conn, stream, Verbosity::Quiet).0
}

/// Build a `ServerDataSource` paired with a mock server thread that ack's
/// `n` incoming requests with a `{status: "ok"}` Map. Use this for tests
/// where validation is expected to fire *after* one or more successful
/// IO round-trips (e.g. the duplicate-label test, where the *second* call
/// must reach the duplicate check).
#[cfg(unix)]
fn make_mocked_data_source(n: usize) -> ServerDataSource {
    let (client, mut server) = UnixStream::pair().unwrap();
    let client_writer = client.try_clone().unwrap();
    let conn = Connection::new(Box::new(client), Box::new(client_writer));
    let stream = conn.new_stream();

    std::thread::spawn(move || {
        for _ in 0..n {
            let Ok(request) = read_packet(&mut server) else {
                return;
            };
            // request_cbor expects a Map response (it calls map_get on it).
            let response = Value::Map(vec![(
                Value::Text("status".into()),
                Value::Text("ok".into()),
            )]);
            let mut payload = Vec::new();
            ciborium::into_writer(&response, &mut payload).unwrap();
            if write_packet(
                &mut server,
                &Packet {
                    stream: request.stream,
                    message_id: request.message_id,
                    is_reply: true,
                    payload,
                },
            )
            .is_err()
            {
                return;
            }
        }
    });

    ServerDataSource::new(conn, stream, Verbosity::Quiet).0
}

// `ServerDataSource::target_observation` validates its input client-side
// before forwarding to the Python server:
//   1. score must be finite (NaN / ±inf rejected).
//   2. each label may be observed at most once per test case.
//
// The tests below build a dead stream (no live peer) so the validation
// panic *must* fire before the would-be `send_request` call.  Without
// the validation the `let _ = send_request(...)` swallows the
// `BrokenPipe` and the function returns normally — no panic.

#[cfg(unix)]
#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_panics_on_nan() {
    let ds = make_dead_data_source();
    ds.target_observation(f64::NAN, "x");
}

#[cfg(unix)]
#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_panics_on_pos_infinity() {
    let ds = make_dead_data_source();
    ds.target_observation(f64::INFINITY, "x");
}

#[cfg(unix)]
#[test]
#[should_panic(expected = "requires a finite score")]
fn target_observation_panics_on_neg_infinity() {
    let ds = make_dead_data_source();
    ds.target_observation(f64::NEG_INFINITY, "x");
}

#[cfg(unix)]
#[test]
#[should_panic(expected = "would overwrite previous tc.target")]
fn target_observation_panics_on_duplicate_label() {
    // Unlike the finite-score tests above, we need the *first* call's
    // send_request to succeed so the second call can reach the duplicate
    // check — a dead stream would panic in send_request before we get there.
    let ds = make_mocked_data_source(1);
    ds.target_observation(1.0, "x");
    ds.target_observation(2.0, "x");
}

#[cfg(unix)]
#[test]
fn target_observation_allows_distinct_labels() {
    // Distinct labels in the same test case should be accepted (not panic).
    // Both calls go through send_request → mock-server (must not flake on a
    // dead stream's server-exited panic, as this test originally did when
    // run in isolation).
    let ds = make_mocked_data_source(2);
    ds.target_observation(1.0, "a");
    ds.target_observation(2.0, "b");
}
