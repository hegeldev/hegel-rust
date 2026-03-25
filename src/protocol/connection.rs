use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use super::channel::Channel;
use super::packet::{Packet, read_packet, write_packet};

pub struct Connection {
    writer: Mutex<UnixStream>,
    /// Per-channel packet senders. The background reader thread dispatches
    /// incoming packets to the appropriate channel's sender.
    channel_senders: Mutex<HashMap<u32, Sender<Packet>>>,
    next_channel_id: AtomicU32,
    server_exited: AtomicBool,
}

impl Connection {
    pub fn new(stream: UnixStream) -> Arc<Self> {
        let reader_stream = stream
            .try_clone()
            .expect("Failed to clone stream for reader thread");

        let conn = Arc::new(Self {
            writer: Mutex::new(stream),
            channel_senders: Mutex::new(HashMap::new()),
            // channel 0 is reserved for the control channel
            next_channel_id: AtomicU32::new(1),
            server_exited: AtomicBool::new(false),
        });

        // Background reader thread: reads all packets from the socket and
        // dispatches them to the appropriate channel's receiver queue.
        let conn_for_reader = Arc::clone(&conn);
        std::thread::spawn(move || {
            loop {
                match read_packet(&mut &reader_stream) {
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
                        // Socket closed or error — mark server as exited and stop.
                        conn_for_reader.server_exited.store(true, Ordering::SeqCst);
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
        let mut stream = self.writer.lock().unwrap();
        match write_packet(&mut *stream, packet) {
            Ok(()) => Ok(()),
            Err(_) if self.server_has_exited() => Err(Self::server_crashed_error()),
            Err(e) => Err(e),
        }
    }
}
