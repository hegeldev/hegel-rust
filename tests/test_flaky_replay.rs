//! Regression test for the client-side flaky-on-replay detection.
//!
//! The client (`run_lifecycle::drive`) performs the final replay from a
//! discovered counterexample's reproduce blob. If that replay does not fail,
//! the test is non-deterministic and the run reports it as flaky. This test
//! pins that detection.

use hegel::generators as gs;
use hegel::{Hegel, Phase, Settings, TestCase, Verbosity};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn replay_of_a_vanishing_failure_is_reported_as_flaky() {
    // The body fails only on its very first invocation. With shrinking and
    // reuse disabled the engine reports that first (failing) example with a
    // reproduce blob; replaying the blob runs the body again, where it passes,
    // so the client detects the test as flaky.
    let body = |tc: TestCase| {
        let _ = tc.draw(gs::booleans());
        let i = CALLS.fetch_add(1, Ordering::SeqCst);
        assert!(i != 0, "fails only on the first call");
    };
    let panic = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(body)
            .settings(
                Settings::new()
                    .phases([Phase::Generate])
                    .database(None)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    }))
    .expect_err("a vanishing failure must surface as a flaky panic");
    let msg = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(msg.contains("Flaky test detected"), "got: {msg:?}");
}
