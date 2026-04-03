use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use ciborium::Value;

use super::connection::Connection;
use super::packet::Packet;
use crate::cbor_utils::{as_text, map_get};
use std::sync::Arc;

const CLOSE_STREAM_PAYLOAD: &[u8] = &[0xFE];
const CLOSE_STREAM_MESSAGE_ID: u32 = (1u32 << 31) - 1;

pub struct Stream {
    pub stream_id: u32,
    connection: Arc<Connection>,
    next_message_id: u32,
    responses: HashMap<u32, Vec<u8>>,
    requests: Vec<Packet>,
    receiver: Receiver<Packet>,
    closed: bool,
}

impl Stream {
    pub(super) fn new(
        stream_id: u32,
        connection: Arc<Connection>,
        receiver: Receiver<Packet>,
    ) -> Self {
        Self {
            stream_id,
            connection,
            next_message_id: 1,
            responses: HashMap::new(),
            requests: Vec::new(),
            receiver,
            closed: false,
        }
    }

    /// Mark this stream as closed without sending a close packet.
    ///
    /// Used when the server has already closed its end (e.g. after overflow).
    pub fn mark_closed(&mut self) {
        self.closed = true;
    }

    fn check_closed(&self) -> std::io::Result<()> {
        if self.closed {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stream is closed",
            ))
        } else {
            Ok(())
        }
    }

    /// Send a request and return the message ID.
    pub fn send_request(&mut self, payload: Vec<u8>) -> std::io::Result<u32> {
        self.check_closed()?;
        let message_id = self.next_message_id;
        self.next_message_id += 1;
        let packet = Packet {
            stream: self.stream_id,
            message_id,
            is_reply: false,
            payload,
        };
        self.connection.send_packet(&packet)?;
        Ok(message_id)
    }

    /// Send a response to a request.
    pub fn write_reply(&self, message_id: u32, payload: Vec<u8>) -> std::io::Result<()> {
        let packet = Packet {
            stream: self.stream_id,
            message_id,
            is_reply: true,
            payload,
        };
        self.connection.send_packet(&packet)
    }

    /// Wait for a response to a previously sent request.
    pub fn receive_reply(&mut self, message_id: u32) -> std::io::Result<Vec<u8>> {
        loop {
            if let Some(payload) = self.responses.remove(&message_id) {
                return Ok(payload);
            }

            self.check_closed()?;
            self.receive_one_packet()?;
        }
    }

    pub fn receive_request(&mut self) -> std::io::Result<(u32, Vec<u8>)> {
        loop {
            if !self.requests.is_empty() {
                let packet = self.requests.remove(0);
                return Ok((packet.message_id, packet.payload));
            }

            self.check_closed()?;
            self.receive_one_packet()?;
        }
    }

    fn receive_one_packet(&mut self) -> std::io::Result<()> {
        let packet = self.receiver.recv().map_err(|_| {
            if self.connection.server_has_exited() {
                std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    super::SERVER_CRASHED_MESSAGE,
                )
            } else {
                std::io::Error::new(std::io::ErrorKind::ConnectionReset, "stream disconnected")
            }
        })?;

        if packet.is_reply {
            self.responses.insert(packet.message_id, packet.payload);
        } else {
            self.requests.push(packet);
        }

        Ok(())
    }

    pub fn close(&mut self) -> std::io::Result<()> {
        self.mark_closed();
        self.connection.unregister_stream(self.stream_id);
        let packet = Packet {
            stream: self.stream_id,
            message_id: CLOSE_STREAM_MESSAGE_ID,
            is_reply: false,
            payload: CLOSE_STREAM_PAYLOAD.to_vec(),
        };
        self.connection.send_packet(&packet)
    }

    pub fn request_cbor(&mut self, message: &Value) -> std::io::Result<Value> {
        let mut payload = Vec::new();
        ciborium::into_writer(message, &mut payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let id = self.send_request(payload)?;
        let response_bytes = self.receive_reply(id)?;

        let response: Value = ciborium::from_reader(&response_bytes[..])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Check for error response
        if let Some(error) = map_get(&response, "error") {
            let error_type = map_get(&response, "type").and_then(as_text).unwrap_or("");
            return Err(std::io::Error::other(format!(
                "Server error ({}): {:?}",
                error_type, error
            )));
        }

        if let Some(result) = map_get(&response, "result") {
            return Ok(result.clone());
        }

        Ok(response)
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.connection.unregister_stream(self.stream_id);
    }
}

#[cfg(test)]
mod tests {
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
}
