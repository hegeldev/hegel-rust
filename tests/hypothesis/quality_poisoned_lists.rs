//! Ported from hypothesis-python/tests/quality/test_poisoned_lists.py.
//!
//! The Python original runs a `ConjectureRunner` directly and reads
//! `runner.interesting_examples` for the shrunk choice sequence; in
//! hegel-rust the `Minimal` helper drives the same native engine end-to-end
//! and returns the replayed minimum. The original's `Poisoned` strategy
//! uses `data.draw_boolean(p)` for rare poisoning, which has no public
//! generator equivalent (`gs::booleans()` takes no probability), so this
//! port goes through `NativeTestCase::weighted(p, None)` via
//! `with_native_tc` and is native-only.
//!
//! The Python file parametrizes over `seed` (4 values); hegel-rust's
//! `Minimal` runs derandomised, so the seed axis collapses. The remaining
//! 3 `size` × 2 `p` × 2 `strategy_class` parametrizations become 12
//! `#[test]` functions below.

#![cfg(feature = "native")]

use hegel::__native_test_internals::native_tc_handle_of;
use hegel::compose;
use hegel::generators::{self as gs, Generator};
use hegel::TestCase;

use crate::common::utils::Minimal;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Poisoned {
    Poison,
    Value(i64),
}

// `STOP_TEST_STRING` is `pub(crate)`, so we reproduce the literal here. A
// panic with this exact message is how the engine signals "end of replay
// buffer" — matches how `tc.draw` propagates `DataSourceError::StopTest`.
const STOP_TEST_STRING: &str = "__HEGEL_STOP_TEST";

fn weighted_boolean(tc: &TestCase, p: f64) -> bool {
    let handle = native_tc_handle_of(tc)
        .expect("weighted_boolean called outside native test context");
    match handle.lock().unwrap().weighted(p, None) {
        Ok(v) => v,
        Err(_) => panic!("{STOP_TEST_STRING}"),
    }
}

fn poisoned(p: f64) -> impl Generator<Poisoned> {
    compose!(|tc| {
        if weighted_boolean(&tc, p) {
            Poisoned::Poison
        } else {
            Poisoned::Value(tc.draw(gs::integers::<i64>().min_value(0).max_value(10)))
        }
    })
}

fn linear_lists(p: f64, size: i64) -> impl Generator<Vec<Poisoned>> {
    let element = poisoned(p);
    compose!(|tc| {
        let length = tc.draw(gs::integers::<i64>().min_value(0).max_value(size));
        (0..length).map(|_| tc.draw(&element)).collect()
    })
}

fn matrices(p: f64, size: i64) -> impl Generator<Vec<Poisoned>> {
    let dim = (size as f64).sqrt().ceil() as i64;
    let element = poisoned(p);
    compose!(|tc| {
        let n = tc.draw(gs::integers::<i64>().min_value(0).max_value(dim));
        let m = tc.draw(gs::integers::<i64>().min_value(0).max_value(dim));
        (0..n * m).map(|_| tc.draw(&element)).collect()
    })
}

fn assert_minimises_to_singleton<G>(strategy: G)
where
    G: Generator<Vec<Poisoned>> + 'static,
{
    let minimal = Minimal::new(strategy, |v: &Vec<Poisoned>| v.contains(&Poisoned::Poison))
        .test_cases(2000)
        .run();
    assert_eq!(minimal, vec![Poisoned::Poison]);
}

#[test]
fn test_minimal_poisoned_linear_lists_size_5_p_0_01() {
    assert_minimises_to_singleton(linear_lists(0.01, 5));
}

#[test]
fn test_minimal_poisoned_linear_lists_size_5_p_0_1() {
    assert_minimises_to_singleton(linear_lists(0.1, 5));
}

#[test]
fn test_minimal_poisoned_linear_lists_size_10_p_0_01() {
    assert_minimises_to_singleton(linear_lists(0.01, 10));
}

#[test]
fn test_minimal_poisoned_linear_lists_size_10_p_0_1() {
    assert_minimises_to_singleton(linear_lists(0.1, 10));
}

#[test]
fn test_minimal_poisoned_linear_lists_size_20_p_0_01() {
    assert_minimises_to_singleton(linear_lists(0.01, 20));
}

#[test]
fn test_minimal_poisoned_linear_lists_size_20_p_0_1() {
    assert_minimises_to_singleton(linear_lists(0.1, 20));
}

#[test]
fn test_minimal_poisoned_matrices_size_5_p_0_01() {
    assert_minimises_to_singleton(matrices(0.01, 5));
}

#[test]
fn test_minimal_poisoned_matrices_size_5_p_0_1() {
    assert_minimises_to_singleton(matrices(0.1, 5));
}

#[test]
fn test_minimal_poisoned_matrices_size_10_p_0_01() {
    assert_minimises_to_singleton(matrices(0.01, 10));
}

#[test]
fn test_minimal_poisoned_matrices_size_10_p_0_1() {
    assert_minimises_to_singleton(matrices(0.1, 10));
}

#[test]
fn test_minimal_poisoned_matrices_size_20_p_0_01() {
    assert_minimises_to_singleton(matrices(0.01, 20));
}

#[test]
fn test_minimal_poisoned_matrices_size_20_p_0_1() {
    assert_minimises_to_singleton(matrices(0.1, 20));
}
