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
                        // Drop all senders so any thread blocked on channel.recv()
                        // unblocks with RecvError instead of hanging forever.
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
        let mut senders = self.channel_senders.lock().unwrap();
        senders.insert(channel_id, tx);
        // If the server already exited, the background reader's clear() either
        // already ran (so our insert is orphaned) or is blocked on this lock
        // (and will clear it). Remove the sender now so recv() unblocks
        // immediately with RecvError.
        if self.server_has_exited() {
            senders.remove(&channel_id);
        }
        drop(senders);
        Channel::new(channel_id, Arc::clone(self), rx)
    }

    pub fn unregister_channel(&self, channel_id: u32) {
        self.channel_senders.lock().unwrap().remove(&channel_id);
    }

    // nocov start
    pub fn mark_server_exited(&self) {
        self.server_exited.store(true, Ordering::SeqCst);
        // nocov end
    }

    pub fn server_has_exited(&self) -> bool {
        self.server_exited.load(Ordering::SeqCst)
    }

    // nocov start
    fn server_crashed_error() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            // nocov end
            super::SERVER_CRASHED_MESSAGE,
        )
    }

    pub fn send_packet(&self, packet: &Packet) -> std::io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        match write_packet(&mut **writer, packet) {
            Ok(()) => Ok(()),
            Err(_) if self.server_has_exited() => Err(Self::server_crashed_error()), // nocov
            Err(e) => Err(e),                                                        // nocov
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    /// Verify that when the reader stream closes (simulating server crash),
    /// channels unblock promptly instead of hanging forever.
    #[test]
    fn test_channel_unblocks_on_reader_close() {
        // Create a connection whose reader returns EOF immediately.
        // This simulates the server process dying.
        let (_, write_end) = UnixStream::pair().unwrap();
        let conn = Connection::new(Box::new(std::io::empty()), Box::new(write_end));

        // Wait for the background reader to detect EOF
        while !conn.server_has_exited() {
            std::thread::yield_now();
        }

        // Channel created AFTER server exit must still unblock, not hang.
        let mut channel = conn.new_channel();

        let result = channel.receive_request();
        assert!(result.is_err());
    }
}
