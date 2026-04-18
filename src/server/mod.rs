// Server backend for Hegel.
//
// When the `native` feature is NOT enabled, this module provides the test
// runner that communicates with a Python hegel-core server over Unix sockets.

pub(crate) mod data_source;
pub(crate) mod process;
pub(crate) mod protocol;
pub mod runner;
pub(crate) mod session;
pub(crate) mod utils;
pub(crate) mod uv;
