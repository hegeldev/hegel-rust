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
use alloc::boxed::Box;
use alloc::format;

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
/// Runs the whole exploration. Suspends only at the offers, so it can be
/// driven with a no-op waker (see [`crate::exchange`]).
///
/// The engine owns the whole exploration — database replay, generation,
/// shrinking, and the final replay of each failure it reports — and every
/// test case is non-final. Each returned
/// [`Failure`](crate::backend::Failure) carries the origin the engine
/// grouped on, a reproduce blob when the failure has one, and the caveat
/// when the run handled nondeterminism. `Err` is a [`RunError`] — a failure
/// of the run itself (health check, nondeterminism) rather than of any test
/// case; the embedding reports it through its own error channel.
pub(crate) async fn run_native_async(
    settings: &Settings,
    database_key: Option<&str>,
    exchange: &CaseExchange,
) -> Result<TestRunResult, RunError> {
    crate::native::test_runner::explore(settings, database_key, exchange).await
}

/// Build a raw [`DataSource`] that replays the choice sequence encoded in a
/// base64 failure blob, or `None` if the blob cannot be decoded (corrupt or
/// from an incompatible Hegel version).
///
/// The replay is a single test case: the embedding caller drives the
/// returned data source directly (generate, spans, targets) and concludes
/// it with [`DataSource::mark_complete`], deciding for itself whether the
/// blob reproduced its failure (the property failed) or is stale (it
/// passed). A deterministic blob replays exactly, and choices that no
/// longer match the caller's generators surface as a stop-test error from
/// the draw that overruns; a nondeterministic blob replays its incumbent
/// timeline with the stored entropy seed and continuation budget, so a
/// diverging replay completes with fresh draws instead.
#[doc(hidden)]
pub fn data_source_for_blob(
    settings: &Settings,
    blob: &str,
) -> Option<Box<dyn DataSource + Send + Sync>> {
    let ntc = match crate::native::blob::decode_blob(blob)? {
        crate::native::blob::DecodedBlob::Choices(choices) => {
            if settings.verbosity == Verbosity::Debug {
                settings.output.line(&format!(
                    "replaying failure blob: choices = {}",
                    choices.len()
                ));
            }
            crate::native::core::NativeTestCase::for_choices(&choices, None, None)
        }
        crate::native::blob::DecodedBlob::Nd(state) => {
            let incumbent = state.incumbent();
            if settings.verbosity == Verbosity::Debug {
                settings.output.line(&format!(
                    "replaying nondeterministic failure blob: choices = {}, pool = {}",
                    incumbent.len(),
                    state.timelines.len() - 1
                ));
            }
            let budget =
                crate::native::core::flattened_values_len(incumbent) + state.extension as usize;
            let rng = crate::native::rng::EngineRng::seeded(state.entropy);
            crate::native::core::NativeTestCase::for_probe(incumbent, rng, budget).ok()?
        }
    };
    ntc.family()
        .set_stateful_step_count(settings.stateful_step_count);
    let (data_source, _handle) = crate::native::data_source::NativeDataSource::new(ntc);
    Some(Box::new(data_source))
}

#[cfg(test)]
#[path = "../tests/embedded/embed_tests.rs"]
mod tests;
