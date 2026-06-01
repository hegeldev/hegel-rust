// In-process Hegel test engine.
//
// When the `native` feature is enabled, this module provides an alternative
// test runner that doesn't require a Python server: a Rust port of
// Hypothesis's conjecture engine (random generation, choice-based shrinking,
// span-mutation, novel-prefix generation) that runs in the same process as
// the user's test.

pub mod base64;
pub mod bignum;
pub mod blob;
pub mod core;
pub mod data_source;
pub mod data_tree;
pub mod database;
pub mod floats;
pub mod intervalsets;
pub mod re;
pub mod schema;
pub mod shrinker;
pub mod statistics;
pub mod targeting;
pub mod test_runner;
