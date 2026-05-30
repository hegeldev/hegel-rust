//! Tests that verify the engine can find counterexamples / interesting values
//! across various domains (arithmetic invariants, collections, floats, mixed
//! types, and historical PBT-smith regressions).

#[path = "../common/mod.rs"]
mod common;

mod arithmetic;
mod collections;
mod floats;
mod pbtsmith_regressions;
mod types;
