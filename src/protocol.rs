//! Binary protocol implementation for Hegel.
//!
//! This module implements the binary packet protocol for communicating with
//! the Hegel server. The protocol uses:
//! - 20-byte binary headers with magic number and CRC32 checksum
//! - CBOR-encoded payloads
//! - Channel multiplexing for concurrent operations

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use ciborium::Value;

use crate::cbor_helpers::{as_text, map_get};

// Protocol constants
const MAGIC: u32 = 0x4845474C; // "HEGL" in big-endian
const HEADER_SIZE: usize = 20;
const REPLY_BIT: u32 = 1 << 31;
const TERMINATOR: u8 = 0x0A;

/// Special payload sent when closing a channel (invalid CBOR byte 0xFE).
const CLOSE_CHANNEL_PAYLOAD: &[u8] = &[0xFE];
/// Special message ID used for channel close packets.
const CLOSE_CHANNEL_MESSAGE_ID: u32 = (1u32 << 31) - 1;

/// Version negotiation message sent by client
pub const VERSION_NEGOTIATION_MESSAGE: &[u8] = b"Hegel/1.0";
/// Expected response for successful version negotiation
pub const VERSION_NEGOTIATION_OK: &[u8] = b"Ok";

/// A packet in the wire protocol.
#[derive(Debug, Clone)]
pub struct Packet {
    pub channel: u32,
    pub message_id: u32,
    pub is_reply: bool,
    pub payload: Vec<u8>,
}

impl Packet {
    /// Create a new request packet.
    pub fn request(channel: u32, message_id: u32, payload: Vec<u8>) -> Self {
        Self {
            channel,
            message_id,
            is_reply: false,
            payload,
        }
    }

    /// Create a new reply packet.
    pub fn reply(channel: u32, message_id: u32, payload: Vec<u8>) -> Self {
        Self {
            channel,
            message_id,
            is_reply: true,
            payload,
        }
    }
}

/// Write a packet to a stream.
pub fn write_packet<W: Write>(writer: &mut W, packet: &Packet) -> std::io::Result<()> {
    let message_id = if packet.is_reply {
        packet.message_id | REPLY_BIT
    } else {
        packet.message_id
    };

    // Build header
    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&MAGIC.to_be_bytes());
    // Checksum placeholder at 4..8, filled after
    header[8..12].copy_from_slice(&packet.channel.to_be_bytes());
    header[12..16].copy_from_slice(&message_id.to_be_bytes());
    header[16..20].copy_from_slice(&(packet.payload.len() as u32).to_be_bytes());

    // Calculate checksum over header (with checksum field as zeros) + payload
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&header);
    hasher.update(&packet.payload);
    let checksum = hasher.finalize();
    header[4..8].copy_from_slice(&checksum.to_be_bytes());

    // Write header + payload + terminator
    writer.write_all(&header)?;
    writer.write_all(&packet.payload)?;
    writer.write_all(&[TERMINATOR])?;
    writer.flush()?;

    Ok(())
}

/// Read a packet from a stream.
pub fn read_packet<R: Read>(reader: &mut R) -> std::io::Result<Packet> {
    // Read header
    let mut header = [0u8; HEADER_SIZE];
    reader.read_exact(&mut header)?;

    let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let checksum = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    let channel = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
    let message_id_raw = u32::from_be_bytes([header[12], header[13], header[14], header[15]]);
    let length = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);

    // Validate magic
    if magic != MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Invalid magic number: expected 0x{:08X}, got 0x{:08X}",
                MAGIC, magic
            ),
        ));
    }

    // Extract reply bit
    let is_reply = message_id_raw & REPLY_BIT != 0;
    let message_id = message_id_raw & !REPLY_BIT;

    // Read payload
    let mut payload = vec![0u8; length as usize];
    reader.read_exact(&mut payload)?;

    // Read terminator
    let mut terminator = [0u8; 1];
    reader.read_exact(&mut terminator)?;
    if terminator[0] != TERMINATOR {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Invalid terminator: expected 0x{:02X}, got 0x{:02X}",
                TERMINATOR, terminator[0]
            ),
        ));
    }

    // Verify checksum
    let mut header_for_check = header;
    header_for_check[4..8].copy_from_slice(&[0, 0, 0, 0]); // Zero out checksum field
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&header_for_check);
    hasher.update(&payload);
    let computed_checksum = hasher.finalize();
    if computed_checksum != checksum {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Checksum mismatch: expected 0x{:08X}, got 0x{:08X}",
                checksum, computed_checksum
            ),
        ));
    }

    Ok(Packet {
        channel,
        message_id,
        is_reply,
        payload,
    })
}

/// A logical channel on a connection.
pub struct Channel {
    pub channel_id: u32,
    connection: Arc<Connection>,
    next_message_id: AtomicU32,
    /// Inbox for received packets (responses and requests)
    responses: Mutex<HashMap<u32, Vec<u8>>>,
    requests: Mutex<VecDeque<Packet>>,
}

impl Channel {
    fn new(channel_id: u32, connection: Arc<Connection>) -> Self {
        Self {
            channel_id,
            connection,
            next_message_id: AtomicU32::new(1),
            responses: Mutex::new(HashMap::new()),
            requests: Mutex::new(VecDeque::new()),
        }
    }

    /// Send a request and return the message ID.
    pub fn send_request(&self, payload: Vec<u8>) -> std::io::Result<u32> {
        let message_id = self.next_message_id.fetch_add(1, Ordering::SeqCst);
        let packet = Packet::request(self.channel_id, message_id, payload);
        self.connection.send_packet(&packet)?;
        Ok(message_id)
    }

    /// Send a response to a request.
    pub fn send_response(&self, message_id: u32, payload: Vec<u8>) -> std::io::Result<()> {
        let packet = Packet::reply(self.channel_id, message_id, payload);
        self.connection.send_packet(&packet)
    }

    /// Wait for a response to a previously sent request.
    pub fn receive_response(&self, message_id: u32) -> std::io::Result<Vec<u8>> {
        loop {
            // Check if we already have the response
            {
                let mut responses = self.responses.lock().unwrap();
                if let Some(payload) = responses.remove(&message_id) {
                    return Ok(payload);
                }
            }

            // Process one message from the connection
            self.process_one_message()?;
        }
    }

    /// Wait for an incoming request.
    pub fn receive_request(&self) -> std::io::Result<(u32, Vec<u8>)> {
        loop {
            // Check if we already have a request
            {
                let mut requests = self.requests.lock().unwrap();
                if let Some(packet) = requests.pop_front() {
                    return Ok((packet.message_id, packet.payload));
                }
            }

            // Process one message from the connection
            self.process_one_message()?;
        }
    }

    /// Process one incoming message and route it appropriately.
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

    /// Close this channel by sending a close packet to the remote side.
    pub fn close(&self) -> std::io::Result<()> {
        let packet = Packet::request(
            self.channel_id,
            CLOSE_CHANNEL_MESSAGE_ID,
            CLOSE_CHANNEL_PAYLOAD.to_vec(),
        );
        self.connection.send_packet(&packet)
    }

    /// Send a CBOR request and wait for a CBOR response.
    pub fn request_cbor(&self, message: &Value) -> std::io::Result<Value> {
        let mut payload = Vec::new();
        ciborium::into_writer(message, &mut payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let id = self.send_request(payload)?;
        let response_bytes = self.receive_response(id)?;

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

        // Extract result field if present
        if let Some(result) = map_get(&response, "result") {
            return Ok(result.clone());
        }

        Ok(response)
    }
}

/// A connection to the Hegel server.
pub struct Connection {
    stream: Mutex<UnixStream>,
    /// Packets that arrived for channels other than the one being processed
    pending_packets: Mutex<HashMap<u32, VecDeque<Packet>>>,
    next_channel_id: AtomicU32,
    channels: Mutex<HashMap<u32, ()>>, // Track which channels exist
}

impl Connection {
    /// Create a new connection from a Unix stream.
    pub fn new(stream: UnixStream) -> Arc<Self> {
        Arc::new(Self {
            stream: Mutex::new(stream),
            pending_packets: Mutex::new(HashMap::new()),
            next_channel_id: AtomicU32::new(1), // 0 is reserved for control
            channels: Mutex::new(HashMap::new()),
        })
    }

    /// Get the control channel (channel 0).
    pub fn control_channel(self: &Arc<Self>) -> Channel {
        Channel::new(0, Arc::clone(self))
    }

    /// Create a new client-side channel with an odd ID (3, 5, 7...).
    pub fn new_channel(self: &Arc<Self>) -> Channel {
        let next = self.next_channel_id.fetch_add(1, Ordering::SeqCst);
        // Client channels use odd IDs: (next << 1) | 1 gives 3, 5, 7, ...
        let channel_id = (next << 1) | 1;
        self.channels.lock().unwrap().insert(channel_id, ());
        Channel::new(channel_id, Arc::clone(self))
    }

    /// Connect to an existing channel (created by the other side).
    pub fn connect_channel(self: &Arc<Self>, channel_id: u32) -> Channel {
        self.channels.lock().unwrap().insert(channel_id, ());
        Channel::new(channel_id, Arc::clone(self))
    }

    /// Send a packet.
    pub fn send_packet(&self, packet: &Packet) -> std::io::Result<()> {
        let mut stream = self.stream.lock().unwrap();
        write_packet(&mut *stream, packet)
    }

    /// Receive a packet for a specific channel.
    /// If a packet for a different channel arrives, it's queued for later.
    pub fn receive_packet_for_channel(&self, channel_id: u32) -> std::io::Result<Packet> {
        // First check pending packets
        {
            let mut pending = self.pending_packets.lock().unwrap();
            if let Some(queue) = pending.get_mut(&channel_id) {
                if let Some(packet) = queue.pop_front() {
                    return Ok(packet);
                }
            }
        }

        // Read from stream until we get a packet for our channel
        loop {
            let packet = {
                let mut stream = self.stream.lock().unwrap();
                read_packet(&mut *stream)?
            };

            if packet.channel == channel_id {
                return Ok(packet);
            }

            // Queue for another channel
            let mut pending = self.pending_packets.lock().unwrap();
            pending.entry(packet.channel).or_default().push_back(packet);
        }
    }

    /// Close the connection.
    pub fn close(&self) -> std::io::Result<()> {
        let stream = self.stream.lock().unwrap();
        stream.shutdown(std::net::Shutdown::Both)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn test_packet_roundtrip() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let packet = Packet::request(1, 42, b"hello world".to_vec());
        write_packet(&mut client, &packet).unwrap();

        let received = read_packet(&mut server).unwrap();
        assert_eq!(received.channel, 1);
        assert_eq!(received.message_id, 42);
        assert!(!received.is_reply);
        assert_eq!(received.payload, b"hello world");
    }

    #[test]
    fn test_reply_packet() {
        let (mut client, mut server) = UnixStream::pair().unwrap();

        let packet = Packet::reply(2, 100, b"response".to_vec());
        write_packet(&mut client, &packet).unwrap();

        let received = read_packet(&mut server).unwrap();
        assert_eq!(received.channel, 2);
        assert_eq!(received.message_id, 100);
        assert!(received.is_reply);
        assert_eq!(received.payload, b"response");
    }

    #[test]
    fn test_cbor_value_roundtrip() {
        use crate::cbor_helpers::cbor_map;
        // Test that ciborium::Value roundtrips through CBOR
        let value = cbor_map! {
            "type" => "integer",
            "min_value" => 0,
            "max_value" => 100
        };

        // Serialize to CBOR bytes
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&value, &mut cbor_bytes).unwrap();

        // Deserialize back
        let back: Value = ciborium::from_reader(&cbor_bytes[..]).unwrap();

        assert_eq!(value, back);
    }

    #[test]
    fn test_cbor_nan_preserved() {
        let value = Value::Float(f64::NAN);
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes).unwrap();
        let back: Value = ciborium::from_reader(&bytes[..]).unwrap();
        if let Value::Float(f) = back {
            assert!(f.is_nan());
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn test_cbor_infinity_preserved() {
        let value = Value::Float(f64::INFINITY);
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes).unwrap();
        let back: Value = ciborium::from_reader(&bytes[..]).unwrap();
        assert_eq!(back, Value::Float(f64::INFINITY));
    }

    #[test]
    fn test_cbor_neg_infinity_preserved() {
        let value = Value::Float(f64::NEG_INFINITY);
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes).unwrap();
        let back: Value = ciborium::from_reader(&bytes[..]).unwrap();
        assert_eq!(back, Value::Float(f64::NEG_INFINITY));
    }
}
