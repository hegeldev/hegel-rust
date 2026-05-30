// In-process Hegel test engine.
//
// This module provides the test runner: a Rust port of Hypothesis's
// conjecture engine (random generation, choice-based shrinking,
// span-mutation, novel-prefix generation) that runs in the same process as
// the user's test.

pub mod bignum;
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
