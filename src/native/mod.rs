// Native pbtkit-style test engine for Hegel.
//
// When the `native` feature is enabled, this module provides an alternative
// test runner that does not require a Python server. Instead, it implements
// the core pbtkit loop (random generation + integrated shrinking) directly
// in Rust.
//
// Based on https://github.com/DRMacIver/pbtkit (core.py).

pub mod bignum;
pub mod cache;
pub mod cathetus;
pub mod choicetree;
pub mod conjecture_runner;
pub mod conjecture_utils;
pub mod core;
pub mod data_source;
pub mod database;
pub mod datatree;
pub mod dynamic_variable;
pub mod featureflags;
pub mod floats;
pub mod intervalsets;
pub mod optimiser;
pub mod re;
pub mod runner;
pub mod schema;
pub mod shrinker;
pub mod tree;
pub mod unicodedata;

use data_source::NativeTestCaseHandle;

/// Return a clone of the native test case handle stored in `tc`, if any.
pub fn native_tc_handle_of(tc: &crate::TestCase) -> Option<NativeTestCaseHandle> {
    tc.native_tc_handle().cloned()
}
