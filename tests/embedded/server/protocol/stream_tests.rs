use super::super::packet::{read_packet, write_packet};
use super::*;
use std::os::unix::net::UnixStream;

#[test]
fn test_operations_on_closed_stream() {
    // Use std::io::empty() so the reader thread exits immediately.
    let (_, write_end) = UnixStream::pair().unwrap();
    let conn = Connection::new(Box::new(std::io::empty()), Box::new(write_end));
    while !conn.server_has_exited() {
        std::thread::yield_now();
    }

    let mut stream = conn.new_stream();
    stream.mark_closed();

    let err = stream.send_request(vec![]).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
}

#[test]
fn test_stream_disconnected_without_server_exit() {
    // Use a real socket so the reader thread blocks (server stays alive).
    let (read_end, write_end) = UnixStream::pair().unwrap();
    let conn = Connection::new(Box::new(read_end), Box::new(write_end));

    let mut stream = conn.new_stream();
    // Drop the sender by unregistering, without server exiting.
    conn.unregister_stream(stream.stream_id);

    let err = stream.receive_request().unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::ConnectionReset);
}

#[test]
fn test_request_cbor_returns_response_without_result_key() {
    let (client, mut server) = UnixStream::pair().unwrap();
    let client_writer = client.try_clone().unwrap();
    let conn = Connection::new(Box::new(client), Box::new(client_writer));
    let mut stream = conn.new_stream();

    // Mock server: read request, send CBOR reply without "error" or "result".
    std::thread::spawn(move || {
        let request = read_packet(&mut server).unwrap();

        let response = Value::Map(vec![(
            Value::Text("status".into()),
            Value::Text("ok".into()),
        )]);
        let mut payload = Vec::new();
        ciborium::into_writer(&response, &mut payload).unwrap();

        let reply = Packet {
            stream: request.stream,
            message_id: request.message_id,
            is_reply: true,
            payload,
        };
        write_packet(&mut server, &reply).unwrap();
    });

    let message = Value::Map(vec![(
        Value::Text("command".into()),
        Value::Text("test".into()),
    )]);
    let result = stream.request_cbor(&message).unwrap();

    if let Value::Map(entries) = &result {
        assert_eq!(entries[0].0, Value::Text("status".into()));
        assert_eq!(entries[0].1, Value::Text("ok".into()));
    } else {
        panic!("expected Map"); // nocov
    }
}
