//! Ported from hypothesis-python/tests/conjecture/test_engine.py.
//!
//! The shrink-quality subset of `test_engine.py` — the
//! `test_can_shrink_variable_draws` / `test_can_shrink_variable_string_draws`
//! / `test_variable_size_string_increasing` /
//! `test_mildly_complicated_strategies` cluster — ports unchanged
//! through the public `minimal(strategy, condition)` API and lives
//! in this file.
//!
//! The remaining ~80 tests assert on `ConjectureRunner` runtime
//! bookkeeping and are ported through the `NativeConjectureRunner`
//! wrapper in `hegel::__native_test_internals`. That wrapper's
//! per-attribute stubs live in `src/native/conjecture_runner.rs`;
//! each port-loop cycle that lands one of those native-gated tests
//! fills in the attribute it touches. See
//! `.claude/skills/porting-tests/SKILL.md` under "`test_engine.py`-shape"
//! for the port path.

#![cfg(feature = "native")]

use crate::common::utils::minimal;
use hegel::TestCase;
use hegel::generators as gs;

// Port of the `@st.composite strategy(draw)` defined inline in
// `test_can_shrink_variable_draws`. Draws a variable `n ∈ [0, 15]` and
// then `n` integer choices. The outer `n` is a separate choice node,
// which is what makes the shrinker's "shrink the length, then each
// element" pattern non-trivial.
#[hegel::composite]
fn variable_int_list(tc: TestCase) -> Vec<i64> {
    let n: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(15));
    let mut result = Vec::with_capacity(n);
    for _ in 0..n {
        result.push(tc.draw(gs::integers::<i64>().min_value(0).max_value(255)));
    }
    result
}

fn check_can_shrink_variable_draws(n_large: i64) {
    let target = 128 * n_large;
    let ints = minimal(variable_int_list(), move |v: &Vec<i64>| {
        v.iter().copied().map(i128::from).sum::<i128>() >= i128::from(target)
    });
    // should look like [target % 255] + [255] * (len - 1)
    let expected_first = target % 255;
    assert_eq!(ints[0], expected_first, "ints = {ints:?}");
    for x in &ints[1..] {
        assert_eq!(*x, 255, "ints = {ints:?}");
    }
}

#[test]
fn test_can_shrink_variable_draws_1() {
    check_can_shrink_variable_draws(1);
}

#[test]
fn test_can_shrink_variable_draws_5() {
    check_can_shrink_variable_draws(5);
}

#[test]
fn test_can_shrink_variable_draws_8() {
    check_can_shrink_variable_draws(8);
}

#[test]
fn test_can_shrink_variable_draws_15() {
    check_can_shrink_variable_draws(15);
}

// Port of the `@st.composite` that draws `n` first, then a string of
// exactly `n` ASCII characters.
#[hegel::composite]
fn variable_ascii_string(tc: TestCase) -> String {
    let n: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(20));
    tc.draw(gs::text().codec("ascii").min_size(n).max_size(n))
}

#[test]
fn test_can_shrink_variable_string_draws() {
    let s = minimal(variable_ascii_string(), |s: &String| {
        s.len() >= 10 && s.contains('a')
    });
    // Upstream TODO_BETTER_SHRINK: ideally `"0" * 9 + "a"`. In practice
    // the shrinker settles on a string matching `0+a`. We mirror that
    // weaker assertion so the test stays faithful to the upstream
    // expectation.
    let matches = s.bytes().all(|b| b == b'0' || b == b'a')
        && s.ends_with('a')
        && s.chars().filter(|c| *c == 'a').count() == 1
        && s.len() >= 2;
    assert!(matches, "s = {s:?}");
}

// Port of the `@st.composite` that inverts `n` so the *strategy's*
// `n` axis shrinks towards 10 rather than 0.
#[hegel::composite]
fn inverted_ascii_string(tc: TestCase) -> String {
    let n_drawn: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(10));
    let n = 10 - n_drawn;
    tc.draw(gs::text().codec("ascii").min_size(n).max_size(n))
}

#[test]
fn test_variable_size_string_increasing() {
    let s = minimal(inverted_ascii_string(), |s: &String| {
        s.len() >= 5 && s.contains('a')
    });
    // Same TODO_BETTER_SHRINK caveat as
    // `test_can_shrink_variable_string_draws`.
    let matches = s.bytes().all(|b| b == b'0' || b == b'a')
        && s.ends_with('a')
        && s.chars().filter(|c| *c == 'a').count() == 1
        && s.len() >= 2;
    assert!(matches, "s = {s:?}");
}

// Coverage tests for engine.py / shrinker.py code paths that are
// exercised by shrinking any mildly-complicated strategy. Upstream is
// a single parametrised test with three rows; the third
// (`st.sampled_from(enum.Flag(...))` → `bit_count(f.value) > 1`) uses
// Python's `enum.Flag` factory which has no direct Rust analogue — it
// builds a 64-bit flag type at runtime. The two `st.lists(...)` rows
// port directly.
#[test]
fn test_mildly_complicated_strategies_integers_list() {
    minimal(
        gs::vecs(gs::integers::<i64>()).min_size(5),
        |_: &Vec<i64>| true,
    );
}

#[test]
fn test_mildly_complicated_strategies_unique_text_list() {
    minimal(
        gs::vecs(gs::text()).min_size(2).unique(true),
        |_: &Vec<String>| true,
    );
}
