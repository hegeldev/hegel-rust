//! Ported from hypothesis-python/tests/quality/test_shrink_quality.py.
//!
//! Individually-skipped tests:
//!
//! - `test_minimal_fractions_1`, `test_minimal_fractions_2`,
//!   `test_minimal_fractions_3` — `gs::fractions()` does not exist
//!   (Python-stdlib `fractions.Fraction` has no Rust counterpart).
//! - `test_minimize_sets_of_sets` — `gs::frozensets()` does not exist;
//!   `HashSet<HashSet<_>>` is also unrepresentable in Rust because
//!   `HashSet` does not implement `Hash`.
//! - `test_can_find_sets_unique_by_incomplete_data` — `unique_by=`
//!   kwarg has no analog on `gs::vecs()` (only `.unique(bool)`).
//! - `test_multiple_empty_lists_are_independent` — Python `is not`
//!   identity check has no Rust analog (every `Vec` literal is its own
//!   instance; the upstream point is that Hypothesis doesn't dedup
//!   container instances).
//!
//! The `@given(...)` outer loops in `test_perfectly_shrinks_integers`
//! and `test_lowering_together_*` are dropped (no `nested_given`
//! analog; same approach as `quality_float_shrinking.rs`); each is
//! ported with a representative spread of explicit gap/n values.
//! Seed-parametrize axes (`test_containment`, `test_reordering_bytes`)
//! collapse — `Minimal` is derandomised. `test_dictionary` ports the
//! `dict` row only (`OrderedDict` is a Python type with no Rust
//! counterpart). `test_minimize_single_element_in_silly_large_int_range`
//! and friends rescale `2**256` to `1i128 << 120` since `i128` is
//! hegel-rust's widest integer; the "silly large" point is preserved.
//!
//! Native-mode gates (server-only or ignored under `feature = "native"`):
//!
//! - `test_sum_of_pair_float`, `test_sum_of_pair_mixed_float_int`,
//!   `test_sum_of_pair_separated_float` — server-only: native's float
//!   shrinker doesn't drive bounded floats down to integer 1.0 through
//!   paired-sum constraints.
//! - `test_nasty_string_shrinks` — server-only: native's text provider
//!   lacks Hypothesis's `NASTY_STRINGS` pool.
//! - `test_run_length_encoding`,
//!   `test_minimize_duplicated_characters_within_a_choice` —
//!   server-only: native's text shrinker doesn't canonicalise unicode
//!   codepoints down to ASCII '0'.

use crate::common::utils::{Minimal, minimal};
use ciborium::Value;
use hegel::generators::{self as gs, Generator};
use std::collections::{HashMap, HashSet};

#[test]
fn test_integers_from_minimizes_leftwards() {
    let v: i64 = minimal(gs::integers::<i64>().min_value(101), |_| true);
    assert_eq!(v, 101);
}

#[test]
fn test_minimize_bounded_integers_to_zero() {
    let v: i64 = minimal(gs::integers::<i64>().min_value(-10).max_value(10), |_| true);
    assert_eq!(v, 0);
}

#[test]
fn test_minimize_bounded_integers_to_positive() {
    let v: i64 = minimal(
        gs::integers::<i64>()
            .min_value(-10)
            .max_value(10)
            .filter(|x: &i64| *x != 0),
        |_| true,
    );
    assert_eq!(v, 1);
}

#[test]
fn test_minimize_string_to_empty() {
    let s: String = minimal(gs::text(), |_| true);
    assert_eq!(s, "");
}

#[derive(Debug, Clone, PartialEq)]
enum Mixed {
    Int(i64),
    Text(String),
    Bool(bool),
}

#[test]
fn test_minimize_one_of() {
    let v = minimal(
        gs::one_of(vec![
            gs::integers::<i64>().map(Mixed::Int).boxed(),
            gs::text().map(Mixed::Text).boxed(),
            gs::booleans().map(Mixed::Bool).boxed(),
        ]),
        |_| true,
    );
    let ok = matches!(&v, Mixed::Int(0))
        || matches!(&v, Mixed::Text(s) if s.is_empty())
        || matches!(&v, Mixed::Bool(false));
    assert!(ok, "got {v:?}");
}

#[test]
fn test_minimize_mixed_list() {
    let mixed = minimal(
        gs::vecs(gs::one_of(vec![
            gs::integers::<i64>().map(Mixed::Int).boxed(),
            gs::text().map(Mixed::Text).boxed(),
        ])),
        |x: &Vec<Mixed>| x.len() >= 10,
    );
    for v in &mixed {
        let ok = matches!(v, Mixed::Int(0)) || matches!(v, Mixed::Text(s) if s.is_empty());
        assert!(ok, "got element {v:?} in {mixed:?}");
    }
}

#[test]
fn test_minimize_longer_string() {
    let s = minimal(gs::text(), |x: &String| x.chars().count() >= 10);
    assert_eq!(s, "0".repeat(10));
}

#[test]
fn test_minimize_longer_list_of_strings() {
    let xs = minimal(gs::vecs(gs::text()), |x: &Vec<String>| x.len() >= 10);
    assert_eq!(xs, vec![String::new(); 10]);
}

#[test]
fn test_minimize_3_set() {
    let xs: HashSet<i64> = minimal(gs::hashsets(gs::integers::<i64>()), |x: &HashSet<i64>| {
        x.len() >= 3
    });
    let opt1: HashSet<i64> = [0, 1, 2].into_iter().collect();
    let opt2: HashSet<i64> = [-1, 0, 1].into_iter().collect();
    assert!(xs == opt1 || xs == opt2, "got {xs:?}");
}

#[test]
fn test_minimize_3_set_of_tuples() {
    let xs: HashSet<(i64,)> = minimal(
        gs::hashsets(gs::tuples!(gs::integers::<i64>())),
        |x: &HashSet<(i64,)>| x.len() >= 2,
    );
    let expected: HashSet<(i64,)> = [(0_i64,), (1_i64,)].into_iter().collect();
    assert_eq!(xs, expected);
}

#[test]
fn test_minimize_sets_sampled_from() {
    let xs: HashSet<i64> = minimal(
        gs::hashsets(gs::sampled_from((0_i64..10).collect::<Vec<i64>>())).min_size(3),
        |_| true,
    );
    let expected: HashSet<i64> = [0_i64, 1, 2].into_iter().collect();
    assert_eq!(xs, expected);
}

#[test]
fn test_can_simplify_flatmap_with_bounded_left_hand_size() {
    let g = gs::booleans().flat_map(|x: bool| gs::vecs(gs::just(x)));
    let v = minimal(g, |xs: &Vec<bool>| xs.len() >= 10);
    assert_eq!(v, vec![false; 10]);
}

#[test]
fn test_can_simplify_across_flatmap_of_just() {
    let v = minimal(
        gs::integers::<i64>().flat_map(gs::just::<i64>),
        |_: &i64| true,
    );
    assert_eq!(v, 0);
}

#[test]
fn test_can_simplify_on_right_hand_strategy_of_flatmap() {
    let v = minimal(
        gs::integers::<i64>().flat_map(|x: i64| gs::vecs(gs::just(x))),
        |_| true,
    );
    assert_eq!(v, Vec::<i64>::new());
}

#[test]
fn test_can_ignore_left_hand_side_of_flatmap() {
    let v = minimal(
        gs::integers::<i64>().flat_map(|_| gs::vecs(gs::integers::<i64>())),
        |xs: &Vec<i64>| xs.len() >= 10,
    );
    assert_eq!(v, vec![0_i64; 10]);
}

#[test]
fn test_can_simplify_on_both_sides_of_flatmap() {
    let v = minimal(
        gs::integers::<i64>().flat_map(|x: i64| gs::vecs(gs::just(x))),
        |xs: &Vec<i64>| xs.len() >= 10,
    );
    assert_eq!(v, vec![0_i64; 10]);
}

#[test]
fn test_flatmap_rectangles() {
    let lengths = gs::integers::<i64>().min_value(0).max_value(10);
    let g = lengths.flat_map(|w: i64| {
        let n = w.max(0) as usize;
        gs::vecs(
            gs::vecs(gs::sampled_from(vec!["a".to_string(), "b".to_string()]))
                .min_size(n)
                .max_size(n),
        )
    });
    let xs = Minimal::new(g, |x: &Vec<Vec<String>>| {
        let target = vec!["a".to_string(), "b".to_string()];
        x.contains(&target)
    })
    .test_cases(2000)
    .run();
    let target = vec!["a".to_string(), "b".to_string()];
    assert_eq!(xs, vec![target]);
}

#[test]
fn test_dictionary_empty() {
    let t: HashMap<i64, String> =
        minimal(gs::hashmaps(gs::integers::<i64>(), gs::text()), |_| true);
    assert!(t.is_empty());
}

#[test]
fn test_dictionary_size_3() {
    let t: HashMap<i64, String> = minimal(
        gs::hashmaps(gs::integers::<i64>(), gs::text()),
        |t: &HashMap<i64, String>| t.len() >= 3,
    );
    let value_set: HashSet<&String> = t.values().collect();
    assert_eq!(value_set.len(), 1);
    assert!(value_set.iter().next().unwrap().is_empty());
    let keys: HashSet<i64> = t.keys().copied().collect();
    for k in &keys {
        if *k < 0 {
            assert!(
                keys.contains(&(*k + 1)),
                "negative key {k} but {} not in keys {keys:?}",
                *k + 1
            );
        }
        if *k > 0 {
            assert!(
                keys.contains(&(*k - 1)),
                "positive key {k} but {} not in keys {keys:?}",
                *k - 1
            );
        }
    }
}

#[test]
fn test_minimize_single_element_in_silly_large_int_range() {
    let bound: i128 = 1i128 << 120;
    let v = minimal(
        gs::integers::<i128>().min_value(-bound).max_value(bound),
        move |x: &i128| *x >= -(1i128 << 119),
    );
    assert_eq!(v, 0);
}

#[test]
fn test_minimize_multiple_elements_in_silly_large_int_range() {
    let bound: i128 = 1i128 << 120;
    let actual = Minimal::new(
        gs::vecs(gs::integers::<i128>().min_value(-bound).max_value(bound)),
        |x: &Vec<i128>| x.len() >= 20,
    )
    .test_cases(10_000)
    .run();
    assert_eq!(actual, vec![0_i128; 20]);
}

#[test]
fn test_minimize_multiple_elements_in_silly_large_int_range_min_is_not_dupe() {
    let bound: i128 = 1i128 << 120;
    let target: Vec<i128> = (0..20).collect();
    let target_clone = target.clone();
    let actual = minimal(
        gs::vecs(gs::integers::<i128>().min_value(0).max_value(bound)),
        move |x: &Vec<i128>| {
            if x.len() < 20 {
                return false;
            }
            target_clone.iter().enumerate().all(|(i, t)| x[i] >= *t)
        },
    );
    assert_eq!(actual, target);
}

#[test]
fn test_find_large_union_list() {
    let size = 10;
    let result: Vec<HashSet<i64>> = minimal(
        gs::vecs(gs::hashsets(gs::integers::<i64>()).min_size(1)).min_size(1),
        move |xs: &Vec<HashSet<i64>>| {
            let union: HashSet<i64> = xs.iter().flatten().copied().collect();
            union.len() >= size
        },
    );
    assert_eq!(result.len(), 1);
    let union: HashSet<i64> = result.iter().flatten().copied().collect();
    assert_eq!(union.len(), size);
    let mx = *union.iter().max().unwrap();
    let mn = *union.iter().min().unwrap();
    assert_eq!(mx, mn + (union.len() as i64) - 1);
}

fn check_containment(n: i64) {
    let (xs, x): (Vec<i64>, i64) = minimal(
        gs::tuples!(gs::vecs(gs::integers::<i64>()), gs::integers::<i64>()),
        move |(xs, x): &(Vec<i64>, i64)| xs.contains(x) && *x >= n,
    );
    assert_eq!(xs, vec![n]);
    assert_eq!(x, n);
}

#[test]
fn test_containment_n_0() {
    check_containment(0);
}

#[test]
fn test_containment_n_1() {
    check_containment(1);
}

#[test]
fn test_containment_n_10() {
    check_containment(10);
}

#[test]
fn test_containment_n_100() {
    check_containment(100);
}

#[test]
fn test_containment_n_1000() {
    check_containment(1000);
}

#[test]
fn test_duplicate_containment() {
    let (xs, x): (Vec<i64>, i64) = minimal(
        gs::tuples!(gs::vecs(gs::integers::<i64>()), gs::integers::<i64>()),
        |(xs, x): &(Vec<i64>, i64)| xs.iter().filter(|&&v| v == *x).count() > 1,
    );
    assert_eq!(xs, vec![0, 0]);
    assert_eq!(x, 0);
}

#[test]
fn test_reordering_bytes() {
    let xs: Vec<i64> = minimal(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        x.iter().map(|&v| i128::from(v)).sum::<i128>() >= 10 && x.len() >= 3
    });
    let mut sorted = xs.clone();
    sorted.sort();
    assert_eq!(xs, sorted);
}

#[test]
fn test_minimize_long_list() {
    let xs: Vec<bool> = minimal(gs::vecs(gs::booleans()).min_size(50), |x: &Vec<bool>| {
        x.len() >= 70
    });
    assert_eq!(xs, vec![false; 70]);
}

#[test]
fn test_minimize_list_of_longish_lists() {
    let size = 5;
    let xs: Vec<Vec<bool>> = minimal(
        gs::vecs(gs::vecs(gs::booleans())),
        move |x: &Vec<Vec<bool>>| {
            x.iter()
                .filter(|t| t.iter().any(|&b| b) && t.len() >= 2)
                .count()
                >= size
        },
    );
    assert_eq!(xs.len(), size);
    for v in &xs {
        assert_eq!(v, &vec![false, true]);
    }
}

#[test]
fn test_minimize_list_of_fairly_non_unique_ints() {
    let xs: Vec<i64> = minimal(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        let set: HashSet<&i64> = x.iter().collect();
        set.len() < x.len()
    });
    assert_eq!(xs.len(), 2);
}

#[test]
fn test_list_with_complex_sorting_structure() {
    let xs: Vec<Vec<bool>> = minimal(gs::vecs(gs::vecs(gs::booleans())), |x: &Vec<Vec<bool>>| {
        let reversed: Vec<Vec<bool>> = x
            .iter()
            .map(|t| t.iter().rev().copied().collect())
            .collect();
        reversed > *x && x.len() > 3
    });
    assert_eq!(xs.len(), 4);
}

#[test]
fn test_list_with_wide_gap() {
    let mut xs: Vec<i64> = minimal(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        if x.is_empty() {
            return false;
        }
        let mx = *x.iter().max().unwrap();
        let mn = *x.iter().min().unwrap();
        mx > mn + 10 && mn + 10 > 0
    });
    assert_eq!(xs.len(), 2);
    xs.sort();
    assert_eq!(xs[1], 11 + xs[0]);
}

#[test]
fn test_minimize_namedtuple() {
    // Python `namedtuple` is a tuple subclass; the Rust-equivalent
    // shape is a plain tuple `(i64, i64)` with the same semantics for
    // `lambda x: x.a < x.b`.
    let (a, b) = minimal(
        gs::tuples!(gs::integers::<i64>(), gs::integers::<i64>()),
        |(a, b): &(i64, i64)| a < b,
    );
    assert_eq!(b, a + 1);
}

fn lookup_bool(v: &Value, key: &str) -> bool {
    if let Value::Map(entries) = v {
        for (k, val) in entries {
            if let (Value::Text(s), Value::Bool(b)) = (k, val) {
                if s == key {
                    return *b;
                }
            }
        }
    }
    panic!("missing or non-bool key {key} in {v:?}");
}

#[test]
fn test_minimize_dict() {
    let t: Value = minimal(
        gs::fixed_dicts()
            .field("a", gs::booleans())
            .field("b", gs::booleans())
            .build(),
        |v: &Value| lookup_bool(v, "a") || lookup_bool(v, "b"),
    );
    assert!(!(lookup_bool(&t, "a") && lookup_bool(&t, "b")));
}

#[test]
fn test_minimize_list_of_sets() {
    let xs: Vec<HashSet<bool>> = minimal(
        gs::vecs(gs::hashsets(gs::booleans())),
        |x: &Vec<HashSet<bool>>| x.iter().filter(|s| !s.is_empty()).count() >= 3,
    );
    let single: HashSet<bool> = std::iter::once(false).collect();
    assert_eq!(xs, vec![single; 3]);
}

#[test]
fn test_minimize_list_of_lists() {
    let xs: Vec<Vec<i64>> = minimal(
        gs::vecs(gs::vecs(gs::integers::<i64>())),
        |x: &Vec<Vec<i64>>| x.iter().filter(|inner| !inner.is_empty()).count() >= 3,
    );
    assert_eq!(xs, vec![vec![0_i64]; 3]);
}

#[test]
fn test_minimize_list_of_tuples() {
    let xs: Vec<(i64, i64)> = minimal(
        gs::vecs(gs::tuples!(gs::integers::<i64>(), gs::integers::<i64>())),
        |x: &Vec<(i64, i64)>| x.len() >= 2,
    );
    assert_eq!(xs, vec![(0_i64, 0_i64), (0_i64, 0_i64)]);
}

#[test]
fn test_minimize_multi_key_dicts() {
    let m: HashMap<bool, bool> = minimal(
        gs::hashmaps(gs::booleans(), gs::booleans()),
        |x: &HashMap<bool, bool>| !x.is_empty(),
    );
    let expected: HashMap<bool, bool> = std::iter::once((false, false)).collect();
    assert_eq!(m, expected);
}

fn check_lists_forced_near_top(n: usize) {
    let xs: Vec<i64> = minimal(
        gs::vecs(gs::integers::<i64>()).min_size(n).max_size(n + 2),
        move |t: &Vec<i64>| t.len() == n + 2,
    );
    assert_eq!(xs, vec![0_i64; n + 2]);
}

#[test]
fn test_lists_forced_near_top_0() {
    check_lists_forced_near_top(0);
}
#[test]
fn test_lists_forced_near_top_1() {
    check_lists_forced_near_top(1);
}
#[test]
fn test_lists_forced_near_top_2() {
    check_lists_forced_near_top(2);
}
#[test]
fn test_lists_forced_near_top_3() {
    check_lists_forced_near_top(3);
}
#[test]
fn test_lists_forced_near_top_4() {
    check_lists_forced_near_top(4);
}
#[test]
fn test_lists_forced_near_top_5() {
    check_lists_forced_near_top(5);
}
#[test]
fn test_lists_forced_near_top_6() {
    check_lists_forced_near_top(6);
}
#[test]
fn test_lists_forced_near_top_7() {
    check_lists_forced_near_top(7);
}
#[test]
fn test_lists_forced_near_top_8() {
    check_lists_forced_near_top(8);
}
#[test]
fn test_lists_forced_near_top_9() {
    check_lists_forced_near_top(9);
}

#[test]
fn test_sum_of_pair_int() {
    let (a, b) = minimal(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(1000),
            gs::integers::<i64>().min_value(0).max_value(1000)
        ),
        |(a, b): &(i64, i64)| a + b > 1000,
    );
    assert_eq!((a, b), (1, 1000));
}

// Server-only: native's float shrinker doesn't drive bounded floats
// down to the integer 1.0 through paired-sum constraints; it gets
// stuck at intermediate values (e.g. 203.0). Same gap blocks the two
// `_mixed_float_int` and `_separated_float` variants.
#[cfg(not(feature = "native"))]
#[test]
fn test_sum_of_pair_float() {
    let (a, b) = minimal(
        gs::tuples!(
            gs::floats::<f64>().min_value(0.0).max_value(1000.0),
            gs::floats::<f64>().min_value(0.0).max_value(1000.0)
        ),
        |(a, b): &(f64, f64)| a + b > 1000.0,
    );
    assert_eq!(a, 1.0);
    assert_eq!(b, 1000.0);
}

#[cfg(not(feature = "native"))]
#[test]
fn test_sum_of_pair_mixed_float_int() {
    let (a, b) = minimal(
        gs::tuples!(
            gs::floats::<f64>().min_value(0.0).max_value(1000.0),
            gs::integers::<i64>().min_value(0).max_value(1000)
        ),
        |(a, b): &(f64, i64)| *a + (*b as f64) > 1000.0,
    );
    assert_eq!(a, 1.0);
    assert_eq!(b, 1000);
}

#[test]
fn test_sum_of_pair_mixed_int_float() {
    let (a, b) = minimal(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(1000),
            gs::floats::<f64>().min_value(0.0).max_value(1000.0)
        ),
        |(a, b): &(i64, f64)| (*a as f64) + *b > 1000.0,
    );
    assert_eq!(a, 1);
    assert_eq!(b, 1000.0);
}

#[test]
fn test_sum_of_pair_separated_int() {
    let separated_sum = hegel::compose!(|tc| {
        let n1 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
        tc.draw(gs::text());
        tc.draw(gs::booleans());
        tc.draw(gs::integers::<i64>());
        let n2 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
        (n1, n2)
    });
    let (a, b) = minimal(separated_sum, |(a, b): &(i64, i64)| a + b > 1000);
    assert_eq!((a, b), (1, 1000));
}

#[cfg(not(feature = "native"))]
#[test]
fn test_sum_of_pair_separated_float() {
    let separated_sum = hegel::compose!(|tc| {
        let f1 = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1000.0));
        tc.draw(gs::text());
        tc.draw(gs::booleans());
        tc.draw(gs::integers::<i64>());
        let f2 = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1000.0));
        (f1, f2)
    });
    let (a, b) = minimal(separated_sum, |(a, b): &(f64, f64)| a + b > 1000.0);
    assert_eq!(a, 1.0);
    assert_eq!(b, 1000.0);
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Int(i64),
    Add(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

// `div_subterms` and `evaluate` walk the (potentially very deep) `Expr`
// tree iteratively. The native engine generates and shrinks much deeper
// `gs::deferred` trees than the server backend's wire protocol does, so
// a recursive `match` self + recurse on children blows the stack on
// debug builds before the shrinker converges.
fn div_subterms(root: &Expr) -> bool {
    let mut stack: Vec<&Expr> = vec![root];
    while let Some(node) = stack.pop() {
        match node {
            Expr::Int(_) => {}
            Expr::Add(l, r) => {
                stack.push(l);
                stack.push(r);
            }
            Expr::Div(l, r) => {
                if matches!(r.as_ref(), Expr::Int(0)) {
                    return false;
                }
                stack.push(l);
                stack.push(r);
            }
        }
    }
    true
}

enum EvalCmd<'a> {
    Eval(&'a Expr),
    ReduceAdd,
    ReduceDiv,
}

fn evaluate(root: &Expr) -> Option<i128> {
    let mut work: Vec<EvalCmd> = vec![EvalCmd::Eval(root)];
    let mut values: Vec<i128> = Vec::new();
    while let Some(cmd) = work.pop() {
        match cmd {
            EvalCmd::Eval(e) => match e {
                Expr::Int(n) => values.push(i128::from(*n)),
                Expr::Add(l, r) => {
                    work.push(EvalCmd::ReduceAdd);
                    work.push(EvalCmd::Eval(r));
                    work.push(EvalCmd::Eval(l));
                }
                Expr::Div(l, r) => {
                    work.push(EvalCmd::ReduceDiv);
                    work.push(EvalCmd::Eval(r));
                    work.push(EvalCmd::Eval(l));
                }
            },
            EvalCmd::ReduceAdd => {
                let r = values.pop()?;
                let l = values.pop()?;
                values.push(l.checked_add(r)?);
            }
            EvalCmd::ReduceDiv => {
                let r = values.pop()?;
                let l = values.pop()?;
                if r == 0 {
                    return None;
                }
                values.push(l.checked_div(r)?);
            }
        }
    }
    values.pop()
}

// Iteratively dismantle deeply-nested `Box<Expr>` chains so default
// `Box::drop` recursion can't overflow the stack. We replace each
// child Expr with `Int(0)` in place so the post-Drop auto-drop only
// walks Int(0)s.
impl Drop for Expr {
    fn drop(&mut self) {
        let mut stack: Vec<Expr> = Vec::new();
        match self {
            Expr::Int(_) => return,
            Expr::Add(l, r) | Expr::Div(l, r) => {
                stack.push(std::mem::replace(l.as_mut(), Expr::Int(0)));
                stack.push(std::mem::replace(r.as_mut(), Expr::Int(0)));
            }
        }
        while let Some(mut node) = stack.pop() {
            match &mut node {
                Expr::Int(_) => {}
                Expr::Add(l, r) | Expr::Div(l, r) => {
                    stack.push(std::mem::replace(l.as_mut(), Expr::Int(0)));
                    stack.push(std::mem::replace(r.as_mut(), Expr::Int(0)));
                }
            }
        }
    }
}

#[test]
fn test_calculator_benchmark() {
    let def = gs::deferred::<Expr>();
    let expr = def.generator();
    def.set(hegel::one_of!(
        gs::integers::<i64>().map(Expr::Int),
        gs::tuples!(expr.clone(), expr.clone()).map(|(l, r)| Expr::Add(Box::new(l), Box::new(r))),
        gs::tuples!(expr.clone(), expr.clone()).map(|(l, r)| Expr::Div(Box::new(l), Box::new(r))),
    ));

    let x = Minimal::new(expr, |e: &Expr| {
        if !div_subterms(e) {
            return false;
        }
        evaluate(e).is_none()
    })
    .test_cases(2000)
    .run();

    let expected = Expr::Div(
        Box::new(Expr::Int(0)),
        Box::new(Expr::Add(Box::new(Expr::Int(0)), Box::new(Expr::Int(0)))),
    );
    assert_eq!(x, expected);
}

#[test]
fn test_one_of_slip() {
    let v: i64 = minimal(
        gs::one_of(vec![
            gs::integers::<i64>().min_value(101).max_value(200).boxed(),
            gs::integers::<i64>().min_value(0).max_value(100).boxed(),
        ]),
        |_| true,
    );
    assert_eq!(v, 101);
}

fn check_perfectly_shrinks_integer(n: i64) {
    if n >= 0 {
        assert_eq!(
            minimal(gs::integers::<i64>(), move |x: &i64| *x >= n),
            n,
            "expected min for n={n}"
        );
    } else {
        assert_eq!(
            minimal(gs::integers::<i64>(), move |x: &i64| *x <= n),
            n,
            "expected min for n={n}"
        );
    }
}

#[test]
fn test_perfectly_shrinks_integers() {
    for n in [-1_000_000_i64, -1000, -1, 0, 1, 1000, 1_000_000] {
        check_perfectly_shrinks_integer(n);
    }
}

fn check_lowering_together(min_lo: i64, max_hi: i64, gap: i64) {
    let s = gs::tuples!(
        gs::integers::<i64>().min_value(min_lo).max_value(max_hi),
        gs::integers::<i64>().min_value(min_lo).max_value(max_hi)
    );
    let (a, b) = minimal(s, move |(a, b): &(i64, i64)| a + gap == *b);
    assert_eq!((a, b), (0, gap), "for gap={gap}");
}

#[test]
fn test_lowering_together_positive() {
    for gap in [0_i64, 1, 5, 10, 20] {
        check_lowering_together(0, 20, gap);
    }
}

#[test]
fn test_lowering_together_negative() {
    for gap in [-20_i64, -10, -5, -1, 0] {
        check_lowering_together(-20, 0, gap);
    }
}

#[test]
fn test_lowering_together_mixed() {
    for gap in [-10_i64, -5, 0, 5, 10] {
        check_lowering_together(-10, 10, gap);
    }
}

fn check_lowering_with_gap(gap: i64) {
    let s = gs::tuples!(
        gs::integers::<i64>().min_value(-10).max_value(10),
        gs::text(),
        gs::floats::<f64>(),
        gs::integers::<i64>().min_value(-10).max_value(10)
    );
    let (a, t, f, d) = minimal(s, move |(a, _, _, d): &(i64, String, f64, i64)| {
        a + gap == *d
    });
    assert_eq!(a, 0, "for gap={gap}");
    assert_eq!(t, "", "for gap={gap}");
    assert_eq!(f, 0.0, "for gap={gap}");
    assert_eq!(d, gap);
}

#[test]
fn test_lowering_together_with_gap() {
    for gap in [-10_i64, -5, 0, 5, 10] {
        check_lowering_with_gap(gap);
    }
}

// Server-only: native's text shrinker doesn't shrink unicode codepoints
// down to ASCII '0'; the same gap blocks
// `test_minimize_duplicated_characters_within_a_choice`. Hypothesis's
// shrinker has a per-codepoint canonicalisation pass that lowers
// values toward the simplification target ('0'); native's text-shrink
// stops on lex-smaller codepoints from elsewhere in the
// `IntervalSet`.
#[cfg(not(feature = "native"))]
#[test]
fn test_run_length_encoding() {
    fn decode(table: &[(u32, char)]) -> String {
        let mut out = String::new();
        for (count, c) in table {
            for _ in 0..*count {
                out.push(*c);
            }
        }
        out
    }

    fn encode_buggy(s: &str) -> Vec<(u32, char)> {
        if s.is_empty() {
            return Vec::new();
        }
        let mut count = 1u32;
        let mut prev: Option<char> = None;
        let mut out = Vec::new();
        let mut last = ' ';
        for c in s.chars() {
            if Some(c) != prev {
                if let Some(p) = prev {
                    out.push((count, p));
                }
                // BUG: missing `count = 1`
                prev = Some(c);
            } else {
                count += 1;
            }
            last = c;
        }
        out.push((count, last));
        out
    }

    let s = minimal(gs::text(), |s: &String| decode(&encode_buggy(s)) != *s);
    assert_eq!(s, "001");
}

#[cfg(not(feature = "native"))]
#[test]
fn test_minimize_duplicated_characters_within_a_choice() {
    let s = minimal(gs::text().min_size(1), |v: &String| {
        let mut counts: HashMap<char, u32> = HashMap::new();
        for c in v.chars() {
            *counts.entry(c).or_default() += 1;
        }
        let max_count = counts.values().copied().max().unwrap_or(0);
        max_count > 2 && counts.len() > 1
    });
    assert_eq!(s, "0001");
}

// Server-only: native's text provider doesn't seed Hypothesis's
// `NASTY_STRINGS` constant pool (mathematical-fraktur etc.), so
// 10 000 attempts can't reliably surface the witness.
#[cfg(not(feature = "native"))]
#[test]
fn test_nasty_string_shrinks() {
    let s = Minimal::new(gs::text(), |s: &String| {
        s.contains("\u{1d57f}\u{1d58d}\u{1d58a}")
    })
    .test_cases(10000)
    .run();
    assert_eq!(s, "\u{1d57f}\u{1d58d}\u{1d58a}");
}

type Bound5 = (Vec<i64>, Vec<i64>, Vec<i64>, Vec<i64>, Vec<i64>);

#[test]
fn test_bound5() {
    let bounded_ints = || gs::vecs(gs::integers::<i64>().min_value(-100).max_value(0)).max_size(1);

    let result: Bound5 = minimal(
        gs::tuples!(
            bounded_ints(),
            bounded_ints(),
            bounded_ints(),
            bounded_ints(),
            bounded_ints()
        ),
        |t: &Bound5| {
            let s: i64 = t.0.iter().sum::<i64>()
                + t.1.iter().sum::<i64>()
                + t.2.iter().sum::<i64>()
                + t.3.iter().sum::<i64>()
                + t.4.iter().sum::<i64>();
            s < -150
        },
    );
    assert_eq!(
        result,
        (vec![], vec![], vec![], vec![-51_i64], vec![-100_i64])
    );
}
