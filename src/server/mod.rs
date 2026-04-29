// Server backend for Hegel: communicates with a Python hegel-core server
// over Unix sockets.

pub(crate) mod data_source;
pub(crate) mod process;
pub(crate) mod protocol;
pub mod runner;
pub(crate) mod session;
pub(crate) mod utils;
pub(crate) mod uv;
