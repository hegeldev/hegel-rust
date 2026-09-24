//! Fails-safe pins for nondeterministic handling, from the deterministic
//! side: a failing property that replays exactly pays a fixed, small number
//! of test-body executions beyond its shrink — the first-interesting check
//! replays and the report-time final replay — and its report is the
//! deterministic one: no caveat, a reproducer line, the test's own panic.

#[path = "common/mod.rs"]
mod common;

use common::utils::{capture_hegel_output, measure_failing_run};
use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase};

/// One boolean draw and an unconditional failure: discovery on the first
/// execution, then the shrink of the boolean (12 executions on the engine
/// before nondeterministic handling, verify and final replay included) plus
/// the four check replays — and nothing else.
#[test]
fn a_deterministic_failure_pays_a_bounded_fixed_overhead() {
    let stats = measure_failing_run(1, 100, |tc| {
        let b = tc.draw(gs::booleans());
        Some(format!("{b}"))
    });
    assert_eq!(stats.calls_at_first_failure, 1);
    assert_eq!(stats.minimal_repr, "false");
    assert!(
        stats.post_discovery_calls() <= 12 + 4,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}

/// No draws at all: nothing to shrink, so the overhead is exactly the four
/// check replays, the shrink phase's opening verify run, and the final
/// replay.
#[test]
fn a_drawless_failure_pays_exactly_the_check_and_final_replays() {
    let stats = measure_failing_run(1, 100, |_tc| Some(String::from("()")));
    assert_eq!(stats.calls_at_first_failure, 1);
    assert_eq!(stats.post_discovery_calls(), 4 + 1 + 1);
}

fn deterministic_fixture(tc: TestCase) {
    let n = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
    assert!(n < 500, "n was {n}");
}

#[test]
fn a_deterministic_failure_reports_without_a_caveat() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(deterministic_fixture)
            .settings(Settings::new().database(None).seed(Some(3)))
            .run();
    });
    let text = lines.join("\n");
    let payload = result.unwrap_err();
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(msg.contains("n was 500"), "got: {msg:?}");
    assert!(!text.contains("note:"), "unexpected caveat in:\n{text}");
    assert!(
        text.contains("#[hegel::reproduce_failure("),
        "expected a reproducer line in:\n{text}"
    );
}
