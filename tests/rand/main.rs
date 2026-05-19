#![cfg(feature = "rand")]

#[path = "../common/mod.rs"]
mod common;

pub use common::not_supported_on_native;

mod randoms;
