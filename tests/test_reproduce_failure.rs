//! `#[hegel::reproduce_failure]` behaviour. The compile-error cases (the
//! attribute on a bare function or above `#[hegel::test]`) live in
//! `tests/ui/reproduce_failure_*.rs`, where trybuild pins their diagnostics.
//!
//! The end-to-end replay test drives `#[ignore]`d fixture tests in this very
//! binary via `exec::self_test`, passing the captured blob through an
//! environment variable — the attribute argument may be any expression, so
//! the fixture reads it back with `std::env::var`.

mod common;

use common::exec::self_test;
use hegel::TestCase;
use hegel::generators as gs;

/// A correct-usage attribute compiles (exercising the `#[hegel::test]`
/// wiring that injects `.reproduce_failure(...)`); at runtime an undecodable
/// blob panics with a clear message rather than passing silently.
#[hegel::test]
#[hegel::reproduce_failure("!!! not a blob !!!")]
#[should_panic(expected = "could not be decoded")]
fn test_reproduce_failure_undecodable_blob_panics(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    let _ = x;
}

const BLOB: &str = "!!! not a blob !!!";

/// The blob argument may be any expression, not just a string literal — e.g.
/// a `const`. Here a `const` with a bogus blob compiles and reaches the
/// runtime decode (which then panics: it can't be decoded).
#[hegel::test]
#[hegel::reproduce_failure(BLOB)]
#[should_panic(expected = "could not be decoded")]
fn test_reproduce_failure_accepts_a_const_blob(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    let _ = x;
}

/// Stage 1 fixture: a failing test with `print_blob = true`, whose output the
/// driver scrapes for the `reproduce_failure("…")` blob.
#[hegel::test(print_blob = true)]
#[ignore = "fixture: run via exec::self_test"]
fn repro_print_blob_fixture(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}

/// Stage 2 fixture: replays the blob passed via `HEGEL_TEST_REPRO_BLOB`.
#[hegel::test]
#[hegel::reproduce_failure(std::env::var("HEGEL_TEST_REPRO_BLOB").unwrap())]
#[ignore = "fixture: run via exec::self_test"]
fn repro_replay_fixture(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}

/// Stage 3 fixture: a stacked stale blob below the good one must not break
/// the replay.
#[hegel::test]
#[hegel::reproduce_failure(std::env::var("HEGEL_TEST_REPRO_BLOB").unwrap())]
#[hegel::reproduce_failure("!!! stale bookkeeping blob !!!")]
#[ignore = "fixture: run via exec::self_test"]
fn repro_replay_stacked_fixture(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    assert!(x < 5, "x was {x}");
}

/// End-to-end: a failing test prints a reproducer blob; pasting that blob
/// into `#[hegel::reproduce_failure(...)]` deterministically reproduces the
/// same failure.
#[test]
fn test_reproduce_failure_replays_real_counterexample() {
    let out = self_test("repro_print_blob_fixture")
        .expect_failure("x was")
        .run();

    let combined = format!("{}\n{}", out.stdout, out.stderr);
    let re = regex::Regex::new(r#"reproduce_failure\("([^"]+)"\)"#).unwrap();
    let blob = re
        .captures(&combined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| panic!("no reproduce_failure line in output:\n{combined}"));

    self_test("repro_replay_fixture")
        .env("HEGEL_TEST_REPRO_BLOB", &blob)
        .expect_failure("x was")
        .run();

    self_test("repro_replay_stacked_fixture")
        .env("HEGEL_TEST_REPRO_BLOB", &blob)
        .expect_failure("x was")
        .run();
}
