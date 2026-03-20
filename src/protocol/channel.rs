use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use ciborium::Value;

use super::connection::Connection;
use super::packet::Packet;
use crate::cbor_utils::{as_text, map_get};

const CLOSE_CHANNEL_PAYLOAD: &[u8] = &[0xFE];
const CLOSE_CHANNEL_MESSAGE_ID: u32 = (1u32 << 31) - 1;

/// A logical channel on a connection.
pub struct Channel {
    pub channel_id: u32,
    connection: Arc<Connection>,
    next_message_id: AtomicU32,
    responses: Mutex<HashMap<u32, Vec<u8>>>,
    requests: Mutex<VecDeque<Packet>>,
    closed: AtomicBool,
}

impl Channel {
    pub(super) fn new(channel_id: u32, connection: Arc<Connection>) -> Self {
        Self {
            channel_id,
            connection,
            next_message_id: AtomicU32::new(1),
            responses: Mutex::new(HashMap::new()),
            requests: Mutex::new(VecDeque::new()),
            closed: AtomicBool::new(false),
        }
    }

    /// Mark this channel as closed without sending a close packet.
    ///
    /// Used when the server has already closed its end (e.g. after overflow).
    pub fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    fn check_closed(&self) -> std::io::Result<()> {
        if self.closed.load(Ordering::SeqCst) {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "channel is closed",
            ))
        } else {
            Ok(())
        }
    }

    /// Send a request and return the message ID.
    pub fn send_request(&self, payload: Vec<u8>) -> std::io::Result<u32> {
        self.check_closed()?;
        let message_id = self.next_message_id.fetch_add(1, Ordering::SeqCst);
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
    pub fn receive_reply(&self, message_id: u32) -> std::io::Result<Vec<u8>> {
        loop {
            // Check if we already have the response
            {
                let mut responses = self.responses.lock().unwrap();
                if let Some(payload) = responses.remove(&message_id) {
                    return Ok(payload);
                }
            }

            self.check_closed()?;

            // Process one message from the connection
            self.process_one_message()?;
        }
    }

    pub fn receive_request(&self) -> std::io::Result<(u32, Vec<u8>)> {
        loop {
            {
                let mut requests = self.requests.lock().unwrap();
                if let Some(packet) = requests.pop_front() {
                    return Ok((packet.message_id, packet.payload));
                }
            }

            self.check_closed()?;

            self.process_one_message()?;
        }
    }

    fn process_one_message(&self) -> std::io::Result<()> {
        let packet = self
            .connection
            .receive_packet_for_channel(self.channel_id)?;

        if packet.is_reply {
            let mut responses = self.responses.lock().unwrap();
            responses.insert(packet.message_id, packet.payload);
        } else {
            let mut requests = self.requests.lock().unwrap();
            requests.push_back(packet);
        }

        Ok(())
    }

    pub fn close(&self) -> std::io::Result<()> {
        self.mark_closed();
        let packet = Packet {
            channel: self.channel_id,
            message_id: CLOSE_CHANNEL_MESSAGE_ID,
            is_reply: false,
            payload: CLOSE_CHANNEL_PAYLOAD.to_vec(),
        };
        self.connection.send_packet(&packet)
    }

    pub fn request_cbor(&self, message: &Value) -> std::io::Result<Value> {
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
