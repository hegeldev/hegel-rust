//! [`NativeRunner`] + [`RunResult`] surface used by the shrinker.
//!
//! `RunResult` is what [`super::test_runner::EngineCtx`] returns from one
//! call to the user's test function, and `NativeRunner` is the
//! object-safe trait the shrinker uses to drive replays.

use crate::native::core::{ChoiceNode, NativeTestCase, Span, Status};

/// One run's worth of results: status, the realised choice nodes and
/// spans, and (for `Status::Interesting`) the panic message that
/// triggered the failure plus an opaque origin string identifying
/// *where* it happened. The origin is supplied by
/// [`crate::run_lifecycle::run_test_case`] from the captured panic
/// `file:line:col`; per-origin shrinking and database storage key on it.
#[derive(Clone)]
pub struct RunResult {
    pub status: Status,
    pub nodes: Vec<ChoiceNode>,
    pub spans: Vec<Span>,
    pub panic_message: Option<String>,
    pub origin: Option<String>,
}

/// Object-safe surface: "run a [`NativeTestCase`] and tell me what
/// happened." [`super::test_runner::EngineCtx`] implements it so the
/// shrinker can drive replays without caring how the runner is wired.
pub trait NativeRunner {
    fn run(&mut self, ntc: NativeTestCase) -> RunResult;
}
