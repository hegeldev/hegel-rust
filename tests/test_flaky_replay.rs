//! Pins how a vanishing failure — one that fails at discovery but never
//! again on replay — surfaces under each nondeterminism strictness. The
//! engine owns the final replay: under the default quiet strictness the
//! failure is reported unconfirmed and the run re-raises the test's own
//! panic; under `Error` the run aborts with the flaky diagnostic.

use hegel::generators as gs;
use hegel::{Hegel, NondeterminismStrictness, Phase, Settings, TestCase, Verbosity};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};

fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("")
}

#[test]
fn a_vanishing_failure_is_still_reported_under_quiet_strictness() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
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
    .expect_err("a vanishing failure is still reported, re-raising its own panic");
    let msg = panic_message(&*panic);
    assert!(msg.contains("fails only on the first call"), "got: {msg:?}");
}

#[test]
fn error_strictness_aborts_a_vanishing_failure_as_flaky() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
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
                    .verbosity(Verbosity::Quiet)
                    .nondeterminism_strictness(NondeterminismStrictness::Error),
            )
            .run();
    }))
    .expect_err("error strictness aborts a vanishing failure");
    let msg = panic_message(&*panic);
    assert!(msg.contains("Flaky test detected"), "got: {msg:?}");
}
