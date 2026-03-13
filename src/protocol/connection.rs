use std::collections::{HashMap, VecDeque};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use super::channel::Channel;
use super::packet::{Packet, read_packet, write_packet};

pub struct Connection {
    stream: Mutex<UnixStream>,
    pending_packets: Mutex<HashMap<u32, VecDeque<Packet>>>,
    next_channel_id: AtomicU32,
    channels: Mutex<HashMap<u32, ()>>,
}

impl Connection {
    pub fn new(stream: UnixStream) -> Arc<Self> {
        Arc::new(Self {
            stream: Mutex::new(stream),
            pending_packets: Mutex::new(HashMap::new()),
            // channel 0 is reserved for the control channel
            next_channel_id: AtomicU32::new(1),
            channels: Mutex::new(HashMap::new()),
        })
    }

    pub fn control_channel(self: &Arc<Self>) -> Channel {
        Channel::new(0, Arc::clone(self))
    }

    pub fn new_channel(self: &Arc<Self>) -> Channel {
        let next = self.next_channel_id.fetch_add(1, Ordering::SeqCst);
        // client channels use odd ids
        let channel_id = (next << 1) | 1;
        self.channels.lock().unwrap().insert(channel_id, ());
        Channel::new(channel_id, Arc::clone(self))
    }

    pub fn connect_channel(self: &Arc<Self>, channel_id: u32) -> Channel {
        self.channels.lock().unwrap().insert(channel_id, ());
        Channel::new(channel_id, Arc::clone(self))
    }

    pub fn send_packet(&self, packet: &Packet) -> std::io::Result<()> {
        let mut stream = self.stream.lock().unwrap();
        write_packet(&mut *stream, packet)
    }

    pub fn receive_packet_for_channel(&self, channel_id: u32) -> std::io::Result<Packet> {
        // check pending packets first
        {
            let mut pending = self.pending_packets.lock().unwrap();
            if let Some(queue) = pending.get_mut(&channel_id) {
                if let Some(packet) = queue.pop_front() {
                    return Ok(packet);
                }
            }
        }

        // then read from stream until we get a packet for our channel, queuing for others
        loop {
            let packet = {
                let mut stream = self.stream.lock().unwrap();
                read_packet(&mut *stream)?
            };

            if packet.channel == channel_id {
                return Ok(packet);
            }

            let mut pending = self.pending_packets.lock().unwrap();
            pending.entry(packet.channel).or_default().push_back(packet);
        }
    }

    pub fn close(&self) -> std::io::Result<()> {
        let stream = self.stream.lock().unwrap();
        stream.shutdown(std::net::Shutdown::Both)
    }
}
