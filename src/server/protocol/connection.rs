use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use super::packet::{Packet, read_packet, write_packet};
use super::stream::Stream;

pub struct Connection {
    writer: Mutex<Box<dyn Write + Send>>,
    /// Per-stream packet senders. The background reader thread dispatches
    /// incoming packets to the appropriate stream's sender.
    stream_senders: Mutex<HashMap<u32, Sender<Packet>>>,
    next_stream_id: AtomicU32,
    server_exited: AtomicBool,
}

impl Connection {
    pub fn new(mut reader: Box<dyn Read + Send>, writer: Box<dyn Write + Send>) -> Arc<Self> {
        let conn = Arc::new(Self {
            writer: Mutex::new(writer),
            stream_senders: Mutex::new(HashMap::new()),
            // stream 0 is reserved for the control stream
            next_stream_id: AtomicU32::new(1),
            server_exited: AtomicBool::new(false),
        });

        // Background reader thread: reads all packets from the stream and
        // dispatches them to the appropriate stream's receiver queue.
        let conn_for_reader = Arc::clone(&conn);
        std::thread::spawn(move || {
            loop {
                match read_packet(&mut reader) {
                    Ok(packet) => {
                        let senders = conn_for_reader.stream_senders.lock().unwrap();
                        if let Some(sender) = senders.get(&packet.stream) {
                            // If the receiver is dropped, the send fails — that's fine,
                            // the stream was closed.
                            let _ = sender.send(packet);
                        }
                        // Packets for unknown streams are silently dropped.
                    }
                    Err(_) => {
                        // Stream closed or error — mark server as exited and stop.
                        conn_for_reader.server_exited.store(true, Ordering::SeqCst);
                        // Drop all senders so any thread blocked on stream.recv()
                        // unblocks with RecvError instead of hanging forever.
                        conn_for_reader.stream_senders.lock().unwrap().clear();
                        break;
                    }
                }
            }
        });

        conn
    }

    pub fn control_stream(self: &Arc<Self>) -> Stream {
        self.register_stream(0)
    }

    pub fn new_stream(self: &Arc<Self>) -> Stream {
        let next = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        // client streams use odd ids
        let stream_id = (next << 1) | 1;
        self.register_stream(stream_id)
    }

    pub fn connect_stream(self: &Arc<Self>, stream_id: u32) -> Stream {
        self.register_stream(stream_id)
    }

    fn register_stream(self: &Arc<Self>, stream_id: u32) -> Stream {
        let (tx, rx) = mpsc::channel();
        let mut senders = self.stream_senders.lock().unwrap();
        senders.insert(stream_id, tx);
        // If the server already exited, the background reader's clear() either
        // already ran (so our insert is orphaned) or is blocked on this lock
        // (and will clear it). Remove the sender now so recv() unblocks
        // immediately with RecvError.
        if self.server_has_exited() {
            senders.remove(&stream_id);
        }
        drop(senders);
        Stream::new(stream_id, Arc::clone(self), rx)
    }

    pub fn unregister_stream(&self, stream_id: u32) {
        self.stream_senders.lock().unwrap().remove(&stream_id);
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

#[cfg(all(test, unix))]
#[path = "../../../tests/embedded/server/protocol/connection_tests.rs"]
mod tests;
