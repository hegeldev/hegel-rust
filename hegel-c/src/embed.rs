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
//! lets them drive it directly. That's what [`run_native_async`] is for:
//! libhegel's C ABI (`hegel_run_start` / `hegel_next_test_case`) drives it
//! one offered test case at a time.

use crate::backend::{DataSource, RunError, TestRunResult};
use crate::exchange::CaseExchange;
use crate::settings::{Settings, Verbosity};

/// Synchronous driver for [`run_native_async`], retained for tests: runs the
/// whole exploration on the calling thread, invoking `run_case` once per
/// test case the engine wants to run.
///
/// `run_case` receives a boxed [`DataSource`](crate::backend::DataSource)
/// for the test case; the callback uses this to generate values, open spans,
/// observe targets, and ultimately call
/// [`DataSource::mark_complete`](crate::backend::DataSource::mark_complete)
/// with the test case's outcome. The callback **must** call `mark_complete`
/// on its data source before returning; the engine reads the outcome back
/// through the data source rather than from the callback's return value.
#[cfg(test)]
pub(crate) fn run_native(
    settings: &Settings,
    database_key: Option<&str>,
    run_case: impl FnMut(Box<dyn DataSource + Send + Sync>),
) -> Result<TestRunResult, RunError> {
    let exchange = CaseExchange::new();
    let run = run_native_async(settings, database_key, &exchange);
    crate::exchange::drive(&exchange, run, run_case)
}

/// Run the native test runner, offering each test case's raw data source to
/// the driver through `exchange`.
///
/// Dispatches on [`Mode`](crate::settings::Mode) and runs the whole
/// exploration. Suspends only at the offers, so it can be driven with a
/// no-op waker (see [`crate::exchange`]).
///
/// The engine only *explores* — database replay, generation, and shrinking —
/// and every test case is non-final. Each returned
/// [`Failure`](crate::backend::Failure) carries the origin the engine grouped
/// on plus a reproduce blob; the caller replays each blob (via
/// `hegel_test_case_from_blob`) to produce the final report and the panic
/// message. `Err` is a [`RunError`] — a failure of the run itself (health
/// check, nondeterminism) rather than of any test case; the embedding reports
/// it through its own error channel.
pub(crate) async fn run_native_async(
    settings: &Settings,
    database_key: Option<&str>,
    exchange: &CaseExchange,
) -> Result<TestRunResult, RunError> {
    if settings.mode == crate::settings::Mode::SingleTestCase {
        let failure =
            crate::native::test_runner::run_single_case(settings, database_key, exchange).await;
        return Ok(TestRunResult {
            failures: failure.into_iter().collect(),
        });
    }

    let failures = crate::native::test_runner::explore(settings, database_key, exchange).await?;
    Ok(TestRunResult { failures })
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
/// [`Verbosity::Debug`](crate::Verbosity::Debug) and the output destination
/// are consulted, logging the decoded choice count — but they intentionally
/// travel with the blob so future settings reach the replay path without a
/// signature break.
#[doc(hidden)]
pub fn data_source_for_blob(
    settings: &Settings,
    blob: &str,
) -> Option<Box<dyn DataSource + Send + Sync>> {
    let choices = crate::native::blob::decode_failure(blob)?;
    if settings.verbosity == Verbosity::Debug {
        settings.output.line(&format!(
            "replaying failure blob: choices = {}",
            choices.len()
        ));
    }
    let ntc = crate::native::core::NativeTestCase::for_choices(&choices, None, None);
    let (data_source, _handle) = crate::native::data_source::NativeDataSource::new(ntc);
    Some(Box::new(data_source))
}

#[cfg(test)]
#[path = "../tests/embedded/embed_tests.rs"]
mod tests;
