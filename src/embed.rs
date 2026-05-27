//! Low-level embedding entry point for the native engine.
//!
//! Hegel's default entry point is [`crate::Hegel::run`], which wraps each
//! test case in a [`crate::TestCase`], catches panics from the test body,
//! and translates them into [`crate::backend::TestCaseResult`] values.
//! That's the right shape for in-process Rust tests where panicking is the
//! natural failure-reporting mechanism.
//!
//! Embedding contexts that don't speak Rust panics — FFI consumers,
//! alternative test harnesses, replay tooling — need a thinner entry point
//! that hands them each test case's raw [`crate::backend::DataSource`] and
//! lets them drive it directly. That's what [`run_native`] is for.
//!
//! Only available with the `native` feature.

use crate::backend::{DataSource, TestRunResult, TestRunner};
use crate::settings::Settings;

/// Drive the native test runner against a callback that receives the raw
/// data source for each test case.
///
/// `run_case` is invoked once per test case the engine wants to run. It
/// receives:
/// - A boxed [`DataSource`] for the test case. The callback uses this to
///   generate values, open spans, observe targets, and ultimately call
///   [`DataSource::mark_complete`] with the test case's outcome.
/// - A `bool` indicating whether this is the *final replay* of a minimal
///   failing example (useful for triggering verbose output on the
///   counterexample only).
///
/// The callback **must** call [`DataSource::mark_complete`] on its data
/// source before returning; the engine reads the outcome back through the
/// data source rather than from the callback's return value.
///
/// Returns the aggregated [`TestRunResult`] describing whether the run
/// passed and listing any distinct failures the engine surfaced.
#[doc(hidden)]
pub fn run_native(
    settings: &Settings,
    database_key: Option<&str>,
    mut run_case: impl FnMut(Box<dyn DataSource + Send + Sync>, bool),
) -> TestRunResult {
    let runner = crate::native::test_runner::NativeTestRunner;
    runner.run(settings, database_key, &mut run_case)
}

#[cfg(test)]
#[path = "../tests/embedded/embed_tests.rs"]
mod tests;
