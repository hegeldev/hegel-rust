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
use crate::settings::{Settings, Verbosity};

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

/// Build a raw [`DataSource`] that replays the choice sequence encoded in a
/// base64 failure blob, or `None` if the blob cannot be decoded (corrupt or
/// from an incompatible Hegel version).
///
/// The replay is a single deterministic test case: the embedding caller
/// drives the returned data source directly (generate, spans, targets) and
/// concludes it with [`DataSource::mark_complete`], deciding for itself
/// whether the blob reproduced its failure (the property failed) or is stale
/// (it passed). A blob whose choices no longer match the caller's generators
/// surfaces as a stop-test error from the draw that overruns.
///
/// `settings` accompany the replay — currently only
/// [`Verbosity::Debug`](crate::Verbosity::Debug) is consulted, logging the
/// decoded choice count — but they intentionally travel with the blob so
/// future settings reach the replay path without a signature break.
#[doc(hidden)]
pub fn data_source_for_blob(
    settings: &Settings,
    blob: &str,
) -> Option<Box<dyn DataSource + Send + Sync>> {
    let choices = crate::native::blob::decode_failure(blob)?;
    if settings.verbosity == Verbosity::Debug {
        eprintln!("replaying failure blob: choices = {}", choices.len());
    }
    let ntc = crate::native::core::NativeTestCase::for_choices(&choices, None, None);
    let (data_source, _handle) = crate::native::data_source::NativeDataSource::new(ntc);
    Some(Box::new(data_source))
}

#[cfg(test)]
#[path = "../tests/embedded/embed_tests.rs"]
mod tests;
