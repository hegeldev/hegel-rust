use std::io::{Read, Write};

const PACKET_MAGIC: u32 = 0x4845474C; // "HEGL" in big-endian
const PACKET_HEADER_SIZE: usize = 20;
const PACKET_TERMINATOR: u8 = 0x0A;
const REPLY_BIT: u32 = 1 << 31;

#[derive(Debug, Clone)]
pub struct Packet {
    pub stream: u32,
    pub message_id: u32,
    pub is_reply: bool,
    pub payload: Vec<u8>,
}

pub fn write_packet<W: Write + ?Sized>(writer: &mut W, packet: &Packet) -> std::io::Result<()> {
    let message_id = if packet.is_reply {
        packet.message_id | REPLY_BIT
    } else {
        packet.message_id
    };

    let mut header = [0u8; PACKET_HEADER_SIZE];
    header[0..4].copy_from_slice(&PACKET_MAGIC.to_be_bytes());
    // Checksum placeholder at 4..8, filled after
    header[8..12].copy_from_slice(&packet.stream.to_be_bytes());
    header[12..16].copy_from_slice(&message_id.to_be_bytes());
    header[16..20].copy_from_slice(&(packet.payload.len() as u32).to_be_bytes());

    // Calculate checksum over header (with checksum field as zeros) + payload
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&header);
    hasher.update(&packet.payload);
    let checksum = hasher.finalize();
    header[4..8].copy_from_slice(&checksum.to_be_bytes());

    writer.write_all(&header)?;
    writer.write_all(&packet.payload)?;
    writer.write_all(&[PACKET_TERMINATOR])?;
    writer.flush()?;

    Ok(())
}

pub fn read_packet<R: Read + ?Sized>(reader: &mut R) -> std::io::Result<Packet> {
    let mut header = [0u8; PACKET_HEADER_SIZE];
    reader.read_exact(&mut header)?;

    let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let checksum = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    let stream = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
    let message_id_raw = u32::from_be_bytes([header[12], header[13], header[14], header[15]]);
    let length = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);

    // nocov start
    if magic != PACKET_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Invalid magic number: expected 0x{:08X}, got 0x{:08X}",
                PACKET_MAGIC, magic
            ),
        ));
    }
    // nocov end

    let is_reply = message_id_raw & REPLY_BIT != 0;
    let message_id = message_id_raw & !REPLY_BIT;

    let mut payload = vec![0u8; length as usize];
    reader.read_exact(&mut payload)?;

    let mut terminator = [0u8; 1];
    reader.read_exact(&mut terminator)?;
    // nocov start
    if terminator[0] != PACKET_TERMINATOR {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Invalid terminator: expected 0x{:02X}, got 0x{:02X}",
                PACKET_TERMINATOR, terminator[0]
            ),
        ));
    }
    // nocov end

    let mut header_for_check = header;
    // zero out checksum field
    header_for_check[4..8].copy_from_slice(&[0, 0, 0, 0]);
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&header_for_check);
    hasher.update(&payload);
    let computed_checksum = hasher.finalize();
    // nocov start
    if computed_checksum != checksum {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Checksum mismatch: expected 0x{:08X}, got 0x{:08X}",
                checksum, computed_checksum
            ),
        ));
    }
    // nocov end

    Ok(Packet {
        stream,
        message_id,
        is_reply,
        payload,
    })
}

#[cfg(all(test, unix))]
#[path = "../../tests/embedded/protocol/packet_tests.rs"]
mod tests;
