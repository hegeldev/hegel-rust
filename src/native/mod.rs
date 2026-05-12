// Native pbtkit-style test engine for Hegel.
//
// When the `native` feature is enabled, this module provides an alternative
// test runner that does not require a Python server. Instead, it implements
// the core pbtkit loop (random generation + integrated shrinking) directly
// in Rust.
//
// Based on https://github.com/DRMacIver/pbtkit (core.py).

pub mod bignum;
pub mod core;
pub mod data_source;
pub mod data_tree;
pub mod database;
pub mod datatree;
pub mod det_tree;
pub mod floats;
pub mod schema;
pub mod shrinker;
pub mod test_runner;
pub mod tree;
