use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use super::channel::Channel;
use super::packet::{Packet, read_packet, write_packet};

pub struct Connection {
    writer: Mutex<Box<dyn Write + Send>>,
    /// Per-channel packet senders. The background reader thread dispatches
    /// incoming packets to the appropriate channel's sender.
    channel_senders: Mutex<HashMap<u32, Sender<Packet>>>,
    next_channel_id: AtomicU32,
    server_exited: AtomicBool,
}

impl Connection {
    pub fn new(mut reader: Box<dyn Read + Send>, writer: Box<dyn Write + Send>) -> Arc<Self> {
        let conn = Arc::new(Self {
            writer: Mutex::new(writer),
            channel_senders: Mutex::new(HashMap::new()),
            // channel 0 is reserved for the control channel
            next_channel_id: AtomicU32::new(1),
            server_exited: AtomicBool::new(false),
        });

        // Background reader thread: reads all packets from the stream and
        // dispatches them to the appropriate channel's receiver queue.
        let conn_for_reader = Arc::clone(&conn);
        std::thread::spawn(move || {
            loop {
                match read_packet(&mut reader) {
                    Ok(packet) => {
                        let senders = conn_for_reader.channel_senders.lock().unwrap();
                        if let Some(sender) = senders.get(&packet.channel) {
                            // If the receiver is dropped, the send fails — that's fine,
                            // the channel was closed.
                            let _ = sender.send(packet);
                        }
                        // Packets for unknown channels are silently dropped.
                    }
                    Err(_) => {
                        // Stream closed or error — mark server as exited and stop.
                        conn_for_reader.server_exited.store(true, Ordering::SeqCst);
                        // Drop all channel senders so that any blocked recv() calls
                        // will return RecvError, unblocking waiting threads.
                        conn_for_reader.channel_senders.lock().unwrap().clear();
                        break;
                    }
                }
            }
        });

        conn
    }

    pub fn control_channel(self: &Arc<Self>) -> Channel {
        self.register_channel(0)
    }

    pub fn new_channel(self: &Arc<Self>) -> Channel {
        let next = self.next_channel_id.fetch_add(1, Ordering::SeqCst);
        // client channels use odd ids
        let channel_id = (next << 1) | 1;
        self.register_channel(channel_id)
    }

    pub fn connect_channel(self: &Arc<Self>, channel_id: u32) -> Channel {
        self.register_channel(channel_id)
    }

    fn register_channel(self: &Arc<Self>, channel_id: u32) -> Channel {
        let (tx, rx) = mpsc::channel();
        self.channel_senders.lock().unwrap().insert(channel_id, tx);
        Channel::new(channel_id, Arc::clone(self), rx)
    }

    pub fn unregister_channel(&self, channel_id: u32) {
        self.channel_senders.lock().unwrap().remove(&channel_id);
    }

    pub fn mark_server_exited(&self) {
        self.server_exited.store(true, Ordering::SeqCst);
    }

    pub fn server_has_exited(&self) -> bool {
        self.server_exited.load(Ordering::SeqCst)
    }

    fn server_crashed_error() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            super::SERVER_CRASHED_MESSAGE,
        )
    }

    pub fn send_packet(&self, packet: &Packet) -> std::io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        match write_packet(&mut **writer, packet) {
            Ok(()) => Ok(()),
            Err(_) if self.server_has_exited() => Err(Self::server_crashed_error()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    fn test_connection() -> Arc<Connection> {
        let (client, server) = UnixStream::pair().unwrap();
        Connection::new(Box::new(server.try_clone().unwrap()), Box::new(client))
    }

    #[test]
    fn test_mark_server_exited() {
        let conn = test_connection();
        assert!(!conn.server_has_exited());
        conn.mark_server_exited();
        assert!(conn.server_has_exited());
    }

    #[test]
    fn test_send_packet_returns_server_crashed_when_exited() {
        // Use a writer that will fail (closed pipe)
        let (read_end, write_end) = UnixStream::pair().unwrap();
        drop(read_end); // close the read end so writes fail
        let conn = Connection::new(Box::new(std::io::empty()), Box::new(write_end));
        conn.mark_server_exited();

        let packet = Packet {
            channel: 0,
            message_id: 1,
            is_reply: false,
            payload: vec![0x42],
        };
        let err = conn.send_packet(&packet).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
        assert!(err.to_string().contains("hegel server process exited"));
    }

    #[test]
    fn test_send_packet_returns_error_when_pipe_broken() {
        let (read_end, write_end) = UnixStream::pair().unwrap();
        drop(read_end);
        let conn = Connection::new(Box::new(std::io::empty()), Box::new(write_end));

        let packet = Packet {
            channel: 0,
            message_id: 1,
            is_reply: false,
            payload: vec![0x42],
        };
        let err = conn.send_packet(&packet).unwrap_err();
        // May be BrokenPipe (write fails first) or ConnectionAborted
        // (background reader detects server exit first) depending on timing
        assert!(
            err.kind() == std::io::ErrorKind::BrokenPipe
                || err.kind() == std::io::ErrorKind::ConnectionAborted,
            "expected BrokenPipe or ConnectionAborted, got {:?}",
            err.kind()
        );
    }
}
