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

/// Convert a `ciborium::Value` to `serde_json::Value`, wrapping NaN/infinity
/// floats as `{"$float": "nan"}` / `{"$float": "inf"}` / `{"$float": "-inf"}`
/// objects so they survive the JSON value model (which cannot represent these).
fn cbor_to_json(value: ciborium::Value) -> serde_json::Value {
    match value {
        ciborium::Value::Null => serde_json::Value::Null,
        ciborium::Value::Bool(b) => serde_json::Value::Bool(b),
        ciborium::Value::Float(f) => {
            if f.is_nan() {
                serde_json::json!({"$float": "nan"})
            } else if f.is_infinite() {
                if f.is_sign_positive() {
                    serde_json::json!({"$float": "inf"})
                } else {
                    serde_json::json!({"$float": "-inf"})
                }
            } else {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
        }
        ciborium::Value::Integer(i) => {
            let n: i128 = i.into();
            if let Ok(v) = u64::try_from(n) {
                serde_json::Value::Number(v.into())
            } else if let Ok(v) = i64::try_from(n) {
                serde_json::Value::Number(v.into())
            } else {
                // Large integer that doesn't fit in i64/u64 — use $integer wrapper
                serde_json::json!({"$integer": n.to_string()})
            }
        }
        ciborium::Value::Text(s) => serde_json::Value::String(s),
        ciborium::Value::Bytes(b) => {
            // Encode bytes as array of numbers (best-effort for JSON)
            serde_json::Value::Array(b.into_iter().map(|byte| byte.into()).collect())
        }
        ciborium::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(cbor_to_json).collect())
        }
        ciborium::Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        ciborium::Value::Text(s) => s,
                        other => format!("{:?}", other),
                    };
                    (key, cbor_to_json(v))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        ciborium::Value::Tag(_, inner) => cbor_to_json(*inner),
        _ => serde_json::Value::Null,
    }
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

    /// Send a JSON request and wait for a JSON response.
    /// Uses serde to serialize directly to CBOR - no manual conversion needed.
    pub fn request_json(&self, message: &serde_json::Value) -> std::io::Result<serde_json::Value> {
        // Serialize JSON value directly to CBOR bytes using serde
        let mut payload = Vec::new();
        ciborium::into_writer(message, &mut payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let id = self.send_request(payload)?;
        let response_bytes = self.receive_response(id)?;

        // Deserialize CBOR to ciborium::Value first, then convert to JSON
        // to preserve NaN/infinity floats as $float wrapper objects.
        let cbor_value: ciborium::Value = ciborium::from_reader(&response_bytes[..])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let response = cbor_to_json(cbor_value);

        // Check for error response
        if let Some(error) = response.get("error") {
            let error_type = response.get("type").and_then(|t| t.as_str()).unwrap_or("");
            return Err(std::io::Error::other(format!(
                "Server error ({}): {}",
                error_type, error
            )));
        }

        // Extract result field if present
        if let Some(result) = response.get("result") {
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
    #[allow(dead_code)]
    pub fn close(&self) -> std::io::Result<()> {
        let stream = self.stream.lock().unwrap();
        stream.shutdown(std::net::Shutdown::Both)
    }
}

/// Perform version negotiation on a connection.
#[allow(dead_code)]
pub fn negotiate_version(connection: &Arc<Connection>) -> std::io::Result<()> {
    let control = connection.control_channel();
    let id = control.send_request(VERSION_NEGOTIATION_MESSAGE.to_vec())?;
    let response = control.receive_response(id)?;

    if response == VERSION_NEGOTIATION_OK {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            format!(
                "Version negotiation failed: {:?}",
                String::from_utf8_lossy(&response)
            ),
        ))
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
    fn test_json_cbor_serde_roundtrip() {
        // Test that serde can roundtrip JSON through CBOR
        let json = serde_json::json!({
            "type": "integer",
            "minimum": 0,
            "maximum": 100
        });

        // Serialize JSON to CBOR bytes
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&json, &mut cbor_bytes).unwrap();

        // Deserialize CBOR bytes back to JSON
        let back: serde_json::Value = ciborium::from_reader(&cbor_bytes[..]).unwrap();

        assert_eq!(json, back);
    }

    #[test]
    fn test_cbor_to_json_nan() {
        let cbor = ciborium::Value::Float(f64::NAN);
        let json = cbor_to_json(cbor);
        assert_eq!(json, serde_json::json!({"$float": "nan"}));
    }

    #[test]
    fn test_cbor_to_json_infinity() {
        let cbor = ciborium::Value::Float(f64::INFINITY);
        let json = cbor_to_json(cbor);
        assert_eq!(json, serde_json::json!({"$float": "inf"}));
    }

    #[test]
    fn test_cbor_to_json_neg_infinity() {
        let cbor = ciborium::Value::Float(f64::NEG_INFINITY);
        let json = cbor_to_json(cbor);
        assert_eq!(json, serde_json::json!({"$float": "-inf"}));
    }

    #[test]
    fn test_cbor_to_json_normal_float() {
        let cbor = ciborium::Value::Float(42.5);
        let json = cbor_to_json(cbor);
        assert_eq!(json, serde_json::json!(42.5));
    }

    #[test]
    fn test_cbor_to_json_nested_nan() {
        let cbor = ciborium::Value::Map(vec![(
            ciborium::Value::Text("result".to_string()),
            ciborium::Value::Float(f64::NAN),
        )]);
        let json = cbor_to_json(cbor);
        assert_eq!(json, serde_json::json!({"result": {"$float": "nan"}}));
    }
}
