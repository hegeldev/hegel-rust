//! Tests asserting that the shrinker produces minimal counterexamples for
//! various generator shapes. Top-level binary mirroring the source pbtkit
//! organisation (one sub-module per topic).

// Many tests in this binary are gated `#[cfg(not(feature = "native"))]` and
// take helper imports with them when the native feature is on, leaving the
// sub-module-level `use ...` lines dangling. The minimal-native port
// deliberately accepts that until the relevant generators land natively.
#![cfg_attr(feature = "native", allow(unused_imports, dead_code))]

#[path = "../common/mod.rs"]
mod common;

pub use common::not_supported_on_native;

mod bytes;
mod collections;
mod composite;
mod flatmap;
mod floats;
mod integers;
mod mixed_types;
mod strings;
