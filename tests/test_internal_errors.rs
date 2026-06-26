//! Violations of Hegel's *own* invariants (a broken internal assertion, a
//! framework bug detected mid-draw) are failures of Hegel, not of the
//! property under test. They must abort the run immediately with a
//! bug-report message — not be classified as a counterexample, shrunk for
//! the full shrink budget, and reported with a reproducer blob.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};

use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase, Verbosity};

#[test]
fn internal_errors_abort_the_run_without_shrinking() {
    let runs = AtomicUsize::new(0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(|tc: TestCase| {
            runs.fetch_add(1, Ordering::SeqCst);
            tc.__draw_named(gs::booleans(), "x", true);
            tc.__draw_named(gs::booleans(), "x", false);
        })
        .settings(
            Settings::new()
                .database(None)
                .derandomize(true)
                .test_cases(50)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    }));

    let payload = result.expect_err("an internal error must fail the run");
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        msg.contains("bug in hegel"),
        "expected the bug-report framing, got: {msg:?}"
    );
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "an internal error must abort the run on the spot, not be shrunk"
    );
}

#[test]
fn repeated_non_repeatable_draw_name_is_an_internal_error() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(|tc: TestCase| {
            tc.__draw_named(gs::booleans(), "y", false);
            tc.__draw_named(gs::booleans(), "y", false);
        })
        .settings(
            Settings::new()
                .database(None)
                .derandomize(true)
                .test_cases(50)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    }));
    let payload = result.expect_err("an internal error must fail the run");
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        msg.contains("used more than once but repeatable is false"),
        "{msg:?}"
    );
    assert!(msg.contains("bug in hegel"), "{msg:?}");
}
