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
pub mod core;
pub mod data_source;
pub mod database;
pub mod dynamic_variable;
pub mod featureflags;
pub mod floats;
pub mod intervalsets;
pub mod re;
pub mod runner;
pub mod schema;
pub mod shrinker;
pub mod tree;
pub mod unicodedata;

use std::cell::RefCell;

use data_source::NativeTestCaseHandle;

thread_local! {
    /// Handle to the `NativeTestCase` for the currently-running test function.
    ///
    /// Set by `tree.rs::execute` for the duration of each test function call so
    /// that native-only primitives (e.g. `FeatureFlags`) can make direct draws
    /// on the underlying test case without going through the `DataSource`
    /// protocol.
    static CURRENT_NATIVE_TC: RefCell<Option<NativeTestCaseHandle>> = const { RefCell::new(None) };
}

/// Install `handle` as the current test-case handle for the duration of `f`.
///
/// Previous value is restored on exit, supporting nested contexts.
pub(crate) fn with_current_native_tc<R>(handle: NativeTestCaseHandle, f: impl FnOnce() -> R) -> R {
    let prev = CURRENT_NATIVE_TC.with(|cell| cell.replace(Some(handle)));
    let result = f();
    CURRENT_NATIVE_TC.with(|cell| cell.replace(prev));
    result
}

/// Run `f` with the current test-case handle, if any.
pub fn with_native_tc<R>(f: impl FnOnce(Option<&NativeTestCaseHandle>) -> R) -> R {
    CURRENT_NATIVE_TC.with(|cell| f(cell.borrow().as_ref()))
}
