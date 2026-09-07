//! The `hegel_*` C ABI as seen by the frontend.
//!
//! Everything the frontend knows about libhegel comes through this module:
//! the opaque handle types, the `repr(C)` enums and structs, and the
//! `hegel_*` functions themselves. For now it re-exports the `hegel_c` rlib.

pub(crate) use hegel_c::*;
