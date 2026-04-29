use super::*;
use std::os::unix::net::UnixStream;

/// Verify that when the reader stream closes (simulating server crash),
/// streams unblock promptly instead of hanging forever.
#[test]
fn test_stream_unblocks_on_reader_close() {
    // Create a connection whose reader returns EOF immediately.
    // This simulates the server process dying.
    let (_, write_end) = UnixStream::pair().unwrap();
    let conn = Connection::new(Box::new(std::io::empty()), Box::new(write_end));

    // Wait for the background reader to detect EOF
    while !conn.server_has_exited() {
        std::thread::yield_now();
    }

    // Stream created AFTER server exit must still unblock, not hang.
    let mut stream = conn.new_stream();

    let result = stream.receive_request();
    assert!(result.is_err());
}
