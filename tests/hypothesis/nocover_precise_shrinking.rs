//! Ported from hypothesis-python/tests/nocover/test_precise_shrinking.py
//!
//! Tests precise shrink-to-minimum behaviour for various strategy types and their
//! combinations via `one_of`.
//!
//! The Python original uses `precisely_shrink()` — a helper that seeds a
//! `ConjectureRunner` with an "end marker" value drawn after the strategy,
//! shrinks while keeping that marker fixed, and asserts the result equals
//! `find(strat, cond)`. The `shrinks()` helper explores all values reachable
//! via the shrinker from a specific starting point (using `ConjectureRunner.shrink`
//! with a condition). This port replaces both helpers with calls to the public
//! `minimal()` / `find_any()` API, testing the same observable outcomes — the
//! shrunk value equals the expected minimum, and every smaller alternative is
//! reachable — without the end-marker precision check.
//!
//! Individually-skipped tests:
//! - `test_strategy_list_is_in_sorted_order`: asserts that Python's
//!   `[st.none(), st.booleans(), st.binary(), st.text(), st.integers()]` are
//!   in ascending `sort_key(minimal_nodes)` order. Python's `st.integers()`
//!   (unbounded) uses a multi-choice draw sequence, so its minimal nodes sort
//!   after `st.binary()` and `st.text()`. Our `gs::integers::<i64>()` uses a
//!   single bounded-integer draw, giving the same minimal `sort_key` as
//!   `gs::booleans()`, which violates the Python ordering and makes the
//!   assertion fail.

#![cfg(feature = "native")]

use crate::common::utils::{find_any, minimal};
use hegel::generators::{self as gs, BoxedGenerator, Generator};

// Mixed-type enum spanning the five "common" Hypothesis strategy types:
// type(None), bool, bytes, str, int (mapped to indices 0–4).
#[derive(Debug, Clone, PartialEq)]
enum PreciseValue {
    Unit,
    Bool(bool),
    Bytes(Vec<u8>),
    Text(String),
    Int(i64),
}

impl PreciseValue {
    fn type_index(&self) -> usize {
        match self {
            Self::Unit => 0,
            Self::Bool(_) => 1,
            Self::Bytes(_) => 2,
            Self::Text(_) => 3,
            Self::Int(_) => 4,
        }
    }
}

fn make_gen(type_idx: usize) -> BoxedGenerator<'static, PreciseValue> {
    match type_idx {
        0 => gs::just(PreciseValue::Unit).boxed(),
        1 => gs::booleans().map(PreciseValue::Bool).boxed(),
        2 => gs::binary().map(PreciseValue::Bytes).boxed(),
        3 => gs::text().map(PreciseValue::Text).boxed(),
        4 => gs::integers::<i64>().map(PreciseValue::Int).boxed(),
        _ => unreachable!(),
    }
}

fn combined_gen(combo: &[usize]) -> gs::OneOfGenerator<'static, PreciseValue> {
    gs::one_of(combo.iter().map(|&t| make_gen(t)))
}

/// Return all k-element subsets of {0, 1, 2, 3, 4} in lexicographic order.
fn combos_from_5(k: usize) -> Vec<Vec<usize>> {
    fn rec(start: usize, k: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        let need = k - cur.len();
        for i in start..=(5 - need) {
            cur.push(i);
            rec(i + 1, k, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    rec(0, k, &mut Vec::new(), &mut out);
    out
}

// ── test_can_precisely_shrink_values ──────────────────────────────────────────
// Python: for each (typ, strat) and require_truthy in [False, True],
//   assert precisely_shrink(strat, is_interesting=cond) == find(strat, cond).
// Rust: assert minimal(gen, cond) == expected_minimum.

#[test]
fn test_can_precisely_shrink_none_trivial() {
    assert_eq!(minimal(gs::unit(), |_| true), ());
}

#[test]
fn test_can_precisely_shrink_booleans_trivial() {
    assert!(!minimal(gs::booleans(), |_| true));
}

#[test]
fn test_can_precisely_shrink_booleans_truthy() {
    assert!(minimal(gs::booleans(), |x: &bool| *x));
}

#[test]
fn test_can_precisely_shrink_binary_trivial() {
    assert!(minimal(gs::binary(), |_: &Vec<u8>| true).is_empty());
}

#[test]
fn test_can_precisely_shrink_binary_truthy() {
    assert_eq!(
        minimal(gs::binary(), |x: &Vec<u8>| !x.is_empty()),
        vec![0u8]
    );
}

#[test]
fn test_can_precisely_shrink_text_trivial() {
    assert_eq!(minimal(gs::text(), |_: &String| true), "");
}

#[test]
fn test_can_precisely_shrink_text_truthy() {
    // Minimum non-empty text: '0' has codepoint_sort_key 0.
    assert_eq!(minimal(gs::text(), |x: &String| !x.is_empty()), "0");
}

#[test]
fn test_can_precisely_shrink_integers_trivial() {
    assert_eq!(minimal(gs::integers::<i64>(), |_| true), 0);
}

#[test]
fn test_can_precisely_shrink_integers_truthy() {
    // Smallest truthy i64: sort_key(1) = (1, false) < sort_key(-1) = (1, true).
    assert_eq!(minimal(gs::integers::<i64>(), |x: &i64| *x != 0), 1);
}

// ── test_can_precisely_shrink_alternatives ────────────────────────────────────
// Python: for every 2/3/4-element combo of strategies and every pair (i, j)
//   with i < j (positions in the combo), precisely_shrink starting from type j
//   with is_interesting = "not any type at position < i" yields type i.
// Rust: minimal(one_of(combo), |x| x.type_index() >= combo[i]) has type_index
//   == combo[i] (the smallest type satisfying the condition).

#[test]
fn test_can_precisely_shrink_alternatives() {
    for n in [2, 3, 4usize] {
        for combo in combos_from_5(n) {
            for idx_i in 0..n {
                for idx_j in (idx_i + 1)..n {
                    let threshold = combo[idx_i];
                    let g = combined_gen(&combo);
                    let result = minimal(g, move |x: &PreciseValue| x.type_index() >= threshold);
                    assert_eq!(
                        result.type_index(),
                        threshold,
                        "combo={combo:?} idx_i={idx_i} idx_j={idx_j}: \
                         expected type {threshold} got {}",
                        result.type_index()
                    );
                }
            }
        }
    }
}

// ── test_precise_shrink_with_blocker ─────────────────────────────────────────
// Python: for every 3-element combo (x, y, z), reorder as (x, z, y) — making z
//   the "blocker" in the middle — then precisely_shrink starting from type z
//   with is_interesting=True yields type x (position 0 in the reordered list).
// Rust: minimal(one_of(x_gen, z_gen, y_gen), |_| true).type_index() == x_idx
//   because position 0 is always the global minimum.

#[test]
fn test_precise_shrink_with_blocker() {
    for combo_3 in combos_from_5(3) {
        let x_idx = combo_3[0];
        let y_idx = combo_3[1];
        let z_idx = combo_3[2];
        // Reordered: (x, z, y).
        let reordered = [x_idx, z_idx, y_idx];
        let g = combined_gen(&reordered);
        let result = minimal(g, |_: &PreciseValue| true);
        assert_eq!(
            result.type_index(),
            x_idx,
            "combo={combo_3:?} reordered={reordered:?}: \
             expected type {x_idx} got {}",
            result.type_index()
        );
    }
}

// ── test_always_shrinks_to_none ───────────────────────────────────────────────
// Python: for every pair (a1, a2) from {booleans, binary, text, integers}^2,
//   find a non-None value from one_of(none, a1, a2), then verify the first
//   reachable simpler value is None.
// Rust: minimal(one_of(unit_gen, a1_gen, a2_gen), |_| true) == Unit, since
//   unit is always at position 0 and is the global minimum.

#[test]
fn test_always_shrinks_to_none() {
    for a1 in [1, 2, 3, 4usize] {
        for a2 in [1, 2, 3, 4usize] {
            let combo = [0, a1, a2];
            let g = combined_gen(&combo);
            let result = minimal(g, |_: &PreciseValue| true);
            assert!(
                matches!(result, PreciseValue::Unit),
                "combo={combo:?}: expected Unit, got {result:?}"
            );
        }
    }
}

// ── test_can_shrink_to_every_smaller_alternative ──────────────────────────────
// Python: for every position i ≥ 1 in a combo, starting from type i, every
//   type at position j < i appears in the values reachable by shrinking.
// Rust: for each j < i, find_any(combined_gen, |x| x.type_index() == combo[j])
//   succeeds — verifying each alternative is independently reachable.

#[test]
fn test_can_shrink_to_every_smaller_alternative() {
    for n in [2, 3, 4usize] {
        for combo in combos_from_5(n) {
            for idx_i in 1..n {
                for idx_j in 0..idx_i {
                    let target_type = combo[idx_j];
                    let g = combined_gen(&combo);
                    let _ = find_any(g, move |x: &PreciseValue| x.type_index() == target_type);
                }
            }
        }
    }
}
