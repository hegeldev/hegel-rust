use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use ciborium::Value;

use super::connection::Connection;
use super::packet::Packet;
use crate::cbor_utils::{as_text, map_get};
use std::sync::Arc;

const CLOSE_CHANNEL_PAYLOAD: &[u8] = &[0xFE];
const CLOSE_CHANNEL_MESSAGE_ID: u32 = (1u32 << 31) - 1;

pub struct Channel {
    pub channel_id: u32,
    connection: Arc<Connection>,
    next_message_id: u32,
    responses: HashMap<u32, Vec<u8>>,
    requests: Vec<Packet>,
    receiver: Receiver<Packet>,
    closed: bool,
}

impl Channel {
    pub(super) fn new(
        channel_id: u32,
        connection: Arc<Connection>,
        receiver: Receiver<Packet>,
    ) -> Self {
        Self {
            channel_id,
            connection,
            next_message_id: 1,
            responses: HashMap::new(),
            requests: Vec::new(),
            receiver,
            closed: false,
        }
    }

    /// Mark this channel as closed without sending a close packet.
    ///
    /// Used when the server has already closed its end (e.g. after overflow).
    pub fn mark_closed(&mut self) {
        self.closed = true;
    }

    fn check_closed(&self) -> std::io::Result<()> {
        if self.closed {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "channel is closed",
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
            channel: self.channel_id,
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
            channel: self.channel_id,
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
                std::io::Error::new(std::io::ErrorKind::ConnectionReset, "channel disconnected")
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
        self.connection.unregister_channel(self.channel_id);
        let packet = Packet {
            channel: self.channel_id,
            message_id: CLOSE_CHANNEL_MESSAGE_ID,
            is_reply: false,
            payload: CLOSE_CHANNEL_PAYLOAD.to_vec(),
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

impl Drop for Channel {
    fn drop(&mut self) {
        self.connection.unregister_channel(self.channel_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn dummy_connection() -> Arc<Connection> {
        use std::os::unix::net::UnixStream;
        let (client, server) = UnixStream::pair().unwrap();
        Connection::new(Box::new(server.try_clone().unwrap()), Box::new(client))
    }

    #[test]
    fn test_send_request_fails_when_closed() {
        let conn = dummy_connection();
        let (_tx, rx) = mpsc::channel();
        let mut ch = Channel::new(42, conn, rx);
        ch.mark_closed();

        let err = ch.send_request(vec![0x01]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
        assert!(err.to_string().contains("channel is closed"));
    }

    #[test]
    fn test_receive_reply_fails_when_closed() {
        let conn = dummy_connection();
        let (_tx, rx) = mpsc::channel();
        let mut ch = Channel::new(42, conn, rx);
        ch.mark_closed();

        let err = ch.receive_reply(1).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn test_receive_one_packet_server_exited() {
        let conn = dummy_connection();
        let (tx, rx) = mpsc::channel();
        let mut ch = Channel::new(42, Arc::clone(&conn), rx);
        conn.mark_server_exited();
        drop(tx); // close sender to make recv fail

        let err = ch.receive_request().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
        assert!(err.to_string().contains("hegel server process exited"));
    }

    #[test]
    fn test_receive_one_packet_channel_disconnected() {
        let conn = dummy_connection();
        let (tx, rx) = mpsc::channel();
        let mut ch = Channel::new(42, conn, rx);
        drop(tx); // close sender but server NOT marked as exited

        let err = ch.receive_request().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionReset);
        assert!(err.to_string().contains("channel disconnected"));
    }

    #[test]
    fn test_request_cbor_returns_response_without_result_key() {
        let conn = dummy_connection();
        let (tx, rx) = mpsc::channel();
        let mut ch = Channel::new(42, conn, rx);

        // Simulate a response that has neither "error" nor "result" keys
        let response = crate::cbor_utils::cbor_map! { "status" => "ok" };
        let mut response_bytes = Vec::new();
        ciborium::into_writer(&response, &mut response_bytes).unwrap();
        tx.send(Packet {
            channel: 42,
            message_id: 1,
            is_reply: true,
            payload: response_bytes,
        })
        .unwrap();

        let result = ch.request_cbor(&crate::cbor_utils::cbor_map! { "cmd" => "test" });
        let val = result.unwrap();
        assert_eq!(map_get(&val, "status").and_then(as_text), Some("ok"));
    }
}
