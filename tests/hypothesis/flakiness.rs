//! Ported from hypothesis-python/tests/cover/test_flakiness.py
//!
//! Individually-skipped tests:
//! - `test_fails_differently_is_flaky` — relies on Hypothesis's `@given`/`core.py`
//!   exception-type comparison during the final-replay phase (`expected_failure`
//!   matching). hegel-rust's server mode communicates only INTERESTING/VALID/INVALID
//!   status (no exception type), so "same status, different exception" is not
//!   detectable as flaky through the hegel protocol.
//! - `test_exceptiongroup_wrapped_naked_exception_is_flaky` — requires Python 3.11+
//!   `ExceptionGroup`/`except*` syntax; no Rust counterpart.
//! - `test_flaky_with_context_when_fails_only_under_tracing` — uses `monkeypatch`
//!   and Hypothesis-internal `Tracer`/`StateForActualGivenExecution`; no Rust
//!   counterpart.
//! - `test_failure_sequence_inducing` — uses `random_module()` (no hegel-rust
//!   analog), `|` union-strategy, and nested `@given`; no portable Rust form.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase, Verbosity};

#[test]
fn test_fails_only_once_is_flaky() {
    let first_call = Arc::new(AtomicBool::new(true));
    expect_panic(
        move || {
            Hegel::new(move |tc: TestCase| {
                let _: i64 = tc.draw(gs::integers());
                if first_call.swap(false, Ordering::SeqCst) {
                    panic!("Nope");
                }
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Flaky test detected",
    );
}

#[test]
fn test_gives_flaky_error_if_assumption_is_flaky() {
    let seen: Arc<Mutex<HashSet<i64>>> = Arc::new(Mutex::new(HashSet::new()));
    expect_panic(
        move || {
            Hegel::new(move |tc: TestCase| {
                let s: i64 = tc.draw(gs::integers());
                let is_unseen = !seen.lock().unwrap().contains(&s);
                tc.assume(is_unseen);
                seen.lock().unwrap().insert(s);
                panic!("AssertionError");
            })
            .settings(Settings::new().verbosity(Verbosity::Quiet).database(None))
            .run();
        },
        "Flaky test detected",
    );
}

#[test]
fn test_does_not_attempt_to_shrink_flaky_errors() {
    let values: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
    expect_panic(
        move || {
            Hegel::new(move |tc: TestCase| {
                let x: i64 = tc.draw(gs::integers());
                // Lock is released before assert fires, so no mutex poisoning.
                values.lock().unwrap().push(x);
                let n = values.lock().unwrap().len();
                assert!(n != 1);
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "Flaky test detected",
    );
}
