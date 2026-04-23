//! Ported from hypothesis-python/tests/nocover/test_baseexception.py
//!
//! The upstream file pins down Hypothesis's distinction between Python
//! `BaseException` subclasses (`KeyboardInterrupt`, `SystemExit`,
//! `GeneratorExit`), which propagate unchanged without replay, and
//! `Exception` subclasses (`ValueError`), which go through the normal
//! catch-shrink-replay cycle — resulting in `Flaky` when the test outcome
//! depends on a counter that can't be reproduced.
//!
//! Rust panics are singular — there's no `BaseException`/`Exception`
//! split. Every panic travels hegel's catch-shrink-replay path, so only
//! the `ValueError` parametrize rows map cleanly onto Rust semantics.
//!
//! Individually-skipped tests (see SKIPPED.md):
//!
//! - `test_exception_propagates_fine[KeyboardInterrupt]`,
//!   `test_exception_propagates_fine[SystemExit]`,
//!   `test_exception_propagates_fine[GeneratorExit]`,
//!   `test_exception_propagates_fine_from_strategy[KeyboardInterrupt]`,
//!   `test_exception_propagates_fine_from_strategy[SystemExit]`,
//!   `test_exception_propagates_fine_from_strategy[GeneratorExit]`,
//!   `test_baseexception_no_rerun_no_flaky[KeyboardInterrupt]`,
//!   `test_baseexception_in_strategy_no_rerun_no_flaky[KeyboardInterrupt]`,
//!   `test_baseexception_in_strategy_no_rerun_no_flaky[SystemExit]`,
//!   `test_baseexception_in_strategy_no_rerun_no_flaky[GeneratorExit]`
//!   — all pin down Python `BaseException`-specific propagation rules.
//!   Rust panics don't distinguish `BaseException` from `Exception`.
//!
//! - `test_explanations` — uses pytest's `testdir` fixture and
//!   `runpytest_inprocess` stdout capture to assert on the stack-trace
//!   explanation printed when `SystemExit`/`GeneratorExit` propagates
//!   out of a `@given` body. Both the `BaseException` trigger and the
//!   pytest-runtime output surface are Python-specific.

use crate::common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Hegel, Settings};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn test_exception_propagates_fine() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _x: i64 = tc.draw(&gs::integers());
                panic!("test_exception_propagates_fine_payload");
            })
            .settings(Settings::new().test_cases(100).database(None))
            .run();
        },
        "test_exception_propagates_fine_payload",
    );
}

#[test]
fn test_exception_propagates_fine_from_strategy() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                // black_box keeps the macro's trailing stop_span/result
                // reachable from the compiler's view. Python's original
                // uses a dead `return draw(none())` for the same reason.
                let _: () = tc.draw(&hegel::compose!(|_tc| {
                    if std::hint::black_box(true) {
                        panic!("test_exception_propagates_fine_from_strategy_payload");
                    }
                }));
            })
            .settings(Settings::new().test_cases(100).database(None))
            .run();
        },
        "test_exception_propagates_fine_from_strategy_payload",
    );
}

#[test]
fn test_baseexception_no_rerun_no_flaky() {
    let runs = Arc::new(AtomicUsize::new(0));
    let runs_outer = Arc::clone(&runs);
    expect_panic(
        move || {
            Hegel::new(move |tc| {
                let _x: i64 = tc.draw(&gs::integers());
                let r = runs_outer.fetch_add(1, Ordering::SeqCst) + 1;
                if r == 3 {
                    panic!("baseexception_no_rerun_payload");
                }
            })
            .settings(Settings::new().test_cases(100).database(None))
            .run();
        },
        "Flaky test detected",
    );
}

#[test]
fn test_baseexception_in_strategy_no_rerun_no_flaky() {
    let runs = Arc::new(AtomicUsize::new(0));
    let runs_outer = Arc::clone(&runs);
    expect_panic(
        move || {
            Hegel::new(move |tc| {
                let runs_gen = Arc::clone(&runs_outer);
                let _: i64 = tc.draw(&hegel::compose!(|tc| {
                    let r = runs_gen.fetch_add(1, Ordering::SeqCst) + 1;
                    if r == 3 {
                        panic!("baseexception_in_strategy_payload");
                    }
                    tc.draw(gs::integers::<i64>())
                }));
            })
            .settings(Settings::new().test_cases(100).database(None))
            .run();
        },
        "Flaky test detected",
    );
}
