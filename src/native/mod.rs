// Native pbtkit-style test engine for Hegel.
//
// When the `native` feature is enabled, this module provides an alternative
// test runner that does not require a Python server. Instead, it implements
// the core pbtkit loop (random generation + integrated shrinking) directly
// in Rust.
//
// Based on https://github.com/DRMacIver/pbtkit (core.py).

pub mod core;
pub mod data_source;
pub mod database;
pub mod runner;
pub mod schema;
pub mod shrinker;
pub mod tree;
