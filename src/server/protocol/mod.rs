mod connection;
mod packet;
mod stream;

pub use connection::Connection;
pub use stream::Stream;

pub const HANDSHAKE_STRING: &[u8] = b"hegel_handshake_start";

pub const SERVER_CRASHED_MESSAGE: &str = "The hegel server process exited unexpectedly. \
     See .hegel/server.log for diagnostic information.";
