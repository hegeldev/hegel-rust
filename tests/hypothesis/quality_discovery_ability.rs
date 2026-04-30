//! Ported from hypothesis-python/tests/quality/test_discovery_ability.py.
//!
//! Statistical tests asserting that generators produce specific values with a
//! minimum probability.  All tests are native-gated; the Python originals use
//! ConjectureRunner directly (no server needed).

#![cfg(feature = "native")]

use std::collections::{HashMap, HashSet};

use hegel::generators::{self as gs, Generator};

use crate::common::utils::find_any;

// ---------------------------------------------------------------------------
// Mixed-type helpers for booleans() | tuples() style generators
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum BoolOrUnit {
    Bool(bool),
    Unit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ThreeWay {
    Bool(bool),
    Unit,
    Str,
}

fn bool_or_unit_gen() -> impl Generator<BoolOrUnit> {
    gs::one_of(vec![
        gs::booleans().map(BoolOrUnit::Bool).boxed(),
        gs::tuples!().map(|_| BoolOrUnit::Unit).boxed(),
    ])
}

fn three_way_gen() -> impl Generator<ThreeWay> {
    gs::one_of(vec![
        gs::booleans().map(ThreeWay::Bool).boxed(),
        gs::tuples!().map(|_| ThreeWay::Unit).boxed(),
        gs::just("hi".to_string()).map(|_| ThreeWay::Str).boxed(),
    ])
}

// Mirrors Python's distorted_value(map(type, x)) for BoolOrUnit lists.
// Checks whether one variant appears 3x or more as often as the other.
fn distorted_by_type_bool_unit(items: &[BoolOrUnit]) -> bool {
    let bools = items
        .iter()
        .filter(|x| matches!(x, BoolOrUnit::Bool(_)))
        .count();
    let units = items.iter().filter(|x| **x == BoolOrUnit::Unit).count();
    let counts: Vec<usize> = [bools, units].into_iter().filter(|&c| c > 0).collect();
    if counts.len() < 2 {
        return false;
    }
    let min = *counts.iter().min().unwrap();
    let max = *counts.iter().max().unwrap();
    min * 3 <= max
}

// Mirrors Python's distorted_value(x) for value-typed lists.
fn distorted_by_value(items: &[i64]) -> bool {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for &v in items {
        *counts.entry(v).or_insert(0) += 1;
    }
    if counts.len() < 2 {
        return false;
    }
    let min = *counts.values().min().unwrap();
    let max = *counts.values().max().unwrap();
    min * 3 <= max
}

fn factorial(n: u32) -> i128 {
    (1..=n as i128).product()
}

// ---------------------------------------------------------------------------
// Integer tests
// ---------------------------------------------------------------------------

#[test]
fn test_can_produce_zero() {
    find_any(gs::integers::<i64>(), |&x| x == 0);
}

#[test]
fn test_can_produce_large_magnitude_integers() {
    find_any(gs::integers::<i64>(), |&x| x.abs() > 1000);
}

#[test]
fn test_can_produce_large_positive_integers() {
    find_any(gs::integers::<i64>(), |&x| x > 1000);
}

#[test]
fn test_can_produce_large_negative_integers() {
    find_any(gs::integers::<i64>(), |&x| x < -1000);
}

#[test]
fn test_can_produce_large_factorial() {
    let factorials: HashSet<i128> = (9..=20u32).map(factorial).collect();
    find_any(gs::integers::<i128>(), move |&x| {
        x >= 50_000 && factorials.contains(&x)
    });
}

#[test]
fn test_can_produce_above_large_factorial() {
    let factorials: HashSet<i128> = (9..=20u32).map(factorial).collect();
    find_any(gs::integers::<i128>(), move |&x| {
        x >= 50_000 && factorials.contains(&(x - 1))
    });
}

#[test]
fn test_can_produce_below_large_factorial() {
    let factorials: HashSet<i128> = (9..=20u32).map(factorial).collect();
    find_any(gs::integers::<i128>(), move |&x| {
        x >= 50_000 && x.checked_add(1).map_or(false, |v| factorials.contains(&v))
    });
}

#[test]
fn test_can_produce_large_factorial_negative() {
    let factorials: HashSet<i128> = (9..=20u32).map(factorial).collect();
    find_any(gs::integers::<i128>(), move |&x| {
        x <= -50_000 && factorials.contains(&x.abs())
    });
}

#[test]
fn test_can_produce_above_large_factorial_negative() {
    let factorials: HashSet<i128> = (9..=20u32).map(factorial).collect();
    find_any(gs::integers::<i128>(), move |&x| {
        x <= -50_000
            && x.checked_sub(1)
                .and_then(|v| v.checked_abs())
                .is_some_and(|a| factorials.contains(&a))
    });
}

#[test]
fn test_can_produce_below_large_factorial_negative() {
    let factorials: HashSet<i128> = (9..=20u32).map(factorial).collect();
    find_any(gs::integers::<i128>(), move |&x| {
        x <= -50_000 && factorials.contains(&(x + 1).abs())
    });
}

// ---------------------------------------------------------------------------
// String tests
// ---------------------------------------------------------------------------

#[test]
fn test_can_produce_unstripped_strings() {
    find_any(gs::text(), |s: &String| s != s.trim());
}

#[test]
fn test_can_produce_stripped_strings() {
    find_any(gs::text(), |s: &String| s == s.trim());
}

#[test]
fn test_can_produce_multi_line_strings() {
    find_any(gs::text(), |s: &String| s.contains('\n'));
}

#[test]
fn test_can_produce_ascii_strings() {
    find_any(gs::text(), |s: &String| s.chars().all(|c| c as u32 <= 127));
}

#[test]
fn test_can_produce_long_strings_with_no_ascii() {
    find_any(gs::text().min_size(5), |s: &String| {
        s.chars().all(|c| c as u32 > 127)
    });
}

#[test]
fn test_can_produce_short_strings_with_some_non_ascii() {
    find_any(gs::text(), |s: &String| {
        s.len() <= 3 && s.chars().any(|c| c as u32 > 127)
    });
}

// ---------------------------------------------------------------------------
// Binary tests
// ---------------------------------------------------------------------------

#[test]
fn test_can_produce_large_binary_strings() {
    find_any(gs::binary(), |b: &Vec<u8>| b.len() > 10);
}

// ---------------------------------------------------------------------------
// Float tests
// ---------------------------------------------------------------------------

#[test]
fn test_can_produce_positive_infinity() {
    find_any(gs::floats::<f64>(), |&x| x == f64::INFINITY);
}

#[test]
fn test_can_produce_negative_infinity() {
    find_any(gs::floats::<f64>(), |&x| x == f64::NEG_INFINITY);
}

#[test]
fn test_can_produce_nan() {
    find_any(gs::floats::<f64>(), |x: &f64| x.is_nan());
}

#[test]
fn test_can_produce_floats_near_left() {
    find_any(gs::floats::<f64>().min_value(0.0).max_value(1.0), |&t| {
        t < 0.2
    });
}

#[test]
fn test_can_produce_floats_near_right() {
    find_any(gs::floats::<f64>().min_value(0.0).max_value(1.0), |&t| {
        t > 0.8
    });
}

#[test]
fn test_can_produce_floats_in_middle() {
    find_any(gs::floats::<f64>().min_value(0.0).max_value(1.0), |&t| {
        (0.2..=0.8).contains(&t)
    });
}

// ---------------------------------------------------------------------------
// List tests
// ---------------------------------------------------------------------------

#[test]
fn test_can_produce_long_lists() {
    find_any(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        x.len() >= 10
    });
}

#[test]
fn test_can_produce_short_lists() {
    find_any(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        x.len() <= 10
    });
}

#[test]
fn test_can_produce_the_same_int_twice() {
    find_any(gs::vecs(gs::integers::<i64>()), |x: &Vec<i64>| {
        x.iter().collect::<HashSet<_>>().len() < x.len()
    });
}

// ---------------------------------------------------------------------------
// sampled_from + set tests
// ---------------------------------------------------------------------------

#[test]
fn test_sampled_from_large_number_can_mix() {
    find_any(
        gs::vecs(gs::sampled_from((0..50i64).collect::<Vec<_>>())).min_size(50),
        |x: &Vec<i64>| x.iter().collect::<HashSet<_>>().len() >= 25,
    );
}

#[test]
fn test_sampled_from_often_distorted() {
    find_any(
        gs::vecs(gs::sampled_from((0..5i64).collect::<Vec<_>>())),
        |x: &Vec<i64>| x.len() >= 3 && distorted_by_value(x),
    );
}

#[test]
fn test_non_empty_subset_of_two_is_usually_large() {
    find_any(
        gs::hashsets(gs::sampled_from(vec![1i64, 2i64])),
        |t: &HashSet<i64>| t.len() == 2,
    );
}

#[test]
fn test_subset_of_ten_is_sometimes_empty() {
    find_any(
        gs::hashsets(gs::integers::<i64>().min_value(1).max_value(10)),
        |t: &HashSet<i64>| t.is_empty(),
    );
}

// ---------------------------------------------------------------------------
// More float/int distribution tests
// ---------------------------------------------------------------------------

#[test]
fn test_mostly_sensible_floats() {
    find_any(gs::floats::<f64>(), |&t| t + 1.0 > t);
}

#[test]
fn test_mostly_largish_floats() {
    find_any(gs::floats::<f64>(), |&t| t > 0.0 && t + 1.0 > 1.0);
}

#[test]
fn test_ints_can_occasionally_be_really_large() {
    find_any(gs::integers::<i128>(), |&t| t >= (1i128 << 63));
}

// ---------------------------------------------------------------------------
// Mixed-type (booleans | tuples) tests
// ---------------------------------------------------------------------------

#[test]
fn test_mixing_is_sometimes_distorted() {
    find_any(gs::vecs(bool_or_unit_gen()), |x: &Vec<BoolOrUnit>| {
        let bools = x
            .iter()
            .filter(|v| matches!(v, BoolOrUnit::Bool(_)))
            .count();
        let units = x.iter().filter(|v| **v == BoolOrUnit::Unit).count();
        bools > 0 && units > 0 && distorted_by_type_bool_unit(x)
    });
}

#[test]
fn test_mixes_2_reasonably_often() {
    find_any(gs::vecs(bool_or_unit_gen()), |x: &Vec<BoolOrUnit>| {
        !x.is_empty()
            && x.iter().any(|v| matches!(v, BoolOrUnit::Bool(_)))
            && x.contains(&BoolOrUnit::Unit)
    });
}

#[test]
fn test_partial_mixes_3_reasonably_often() {
    find_any(gs::vecs(three_way_gen()), |x: &Vec<ThreeWay>| {
        let has_bool = x.iter().any(|v| matches!(v, ThreeWay::Bool(_)));
        let has_unit = x.contains(&ThreeWay::Unit);
        let has_str = x.contains(&ThreeWay::Str);
        let distinct = [has_bool, has_unit, has_str]
            .into_iter()
            .filter(|&b| b)
            .count();
        !x.is_empty() && distinct > 1 && distinct < 3
    });
}

#[test]
fn test_mixes_not_too_often() {
    find_any(gs::vecs(bool_or_unit_gen()), |x: &Vec<BoolOrUnit>| {
        !x.is_empty() && {
            let has_bool = x.iter().any(|v| matches!(v, BoolOrUnit::Bool(_)));
            let has_unit = x.contains(&BoolOrUnit::Unit);
            !(has_bool && has_unit)
        }
    });
}

// ---------------------------------------------------------------------------
// More integer distribution tests
// ---------------------------------------------------------------------------

#[test]
fn test_integers_are_usually_non_zero() {
    find_any(gs::integers::<i64>(), |&x| x != 0);
}

#[test]
fn test_integers_are_sometimes_zero() {
    find_any(gs::integers::<i64>(), |&x| x == 0);
}

#[test]
fn test_integers_are_often_small() {
    find_any(gs::integers::<i64>(), |&x| x.saturating_abs() <= 100);
}

#[test]
fn test_integers_are_often_small_but_not_that_small() {
    find_any(gs::integers::<i64>(), |&x| {
        (50..=255).contains(&x.saturating_abs())
    });
}

// ---------------------------------------------------------------------------
// one_of flattening / branch coverage tests
// ---------------------------------------------------------------------------

fn one_of_nested_strategy() -> impl Generator<i64> {
    gs::one_of(vec![
        gs::just(0i64).boxed(),
        gs::one_of(vec![
            gs::just(1i64).boxed(),
            gs::just(2i64).boxed(),
            gs::one_of(vec![
                gs::just(3i64).boxed(),
                gs::just(4i64).boxed(),
                gs::one_of(vec![
                    gs::just(5i64).boxed(),
                    gs::just(6i64).boxed(),
                    gs::just(7i64).boxed(),
                ])
                .boxed(),
            ])
            .boxed(),
        ])
        .boxed(),
    ])
}

#[test]
fn test_one_of_flattens_branches_0() {
    find_any(one_of_nested_strategy(), |&x| x == 0);
}

#[test]
fn test_one_of_flattens_branches_1() {
    find_any(one_of_nested_strategy(), |&x| x == 1);
}

#[test]
fn test_one_of_flattens_branches_2() {
    find_any(one_of_nested_strategy(), |&x| x == 2);
}

#[test]
fn test_one_of_flattens_branches_3() {
    find_any(one_of_nested_strategy(), |&x| x == 3);
}

#[test]
fn test_one_of_flattens_branches_4() {
    find_any(one_of_nested_strategy(), |&x| x == 4);
}

#[test]
fn test_one_of_flattens_branches_5() {
    find_any(one_of_nested_strategy(), |&x| x == 5);
}

#[test]
fn test_one_of_flattens_branches_6() {
    find_any(one_of_nested_strategy(), |&x| x == 6);
}

#[test]
fn test_one_of_flattens_branches_7() {
    find_any(one_of_nested_strategy(), |&x| x == 7);
}

// xor_nested_strategy uses | operator (left-associative): same values {0..7},
// different nesting structure.
fn xor_nested_strategy() -> impl Generator<i64> {
    gs::one_of(vec![
        gs::just(0i64).boxed(),
        gs::one_of(vec![
            gs::one_of(vec![gs::just(1i64).boxed(), gs::just(2i64).boxed()]).boxed(),
            gs::one_of(vec![
                gs::one_of(vec![gs::just(3i64).boxed(), gs::just(4i64).boxed()]).boxed(),
                gs::one_of(vec![
                    gs::one_of(vec![gs::just(5i64).boxed(), gs::just(6i64).boxed()]).boxed(),
                    gs::just(7i64).boxed(),
                ])
                .boxed(),
            ])
            .boxed(),
        ])
        .boxed(),
    ])
}

#[test]
fn test_xor_flattens_branches_0() {
    find_any(xor_nested_strategy(), |&x| x == 0);
}

#[test]
fn test_xor_flattens_branches_1() {
    find_any(xor_nested_strategy(), |&x| x == 1);
}

#[test]
fn test_xor_flattens_branches_2() {
    find_any(xor_nested_strategy(), |&x| x == 2);
}

#[test]
fn test_xor_flattens_branches_3() {
    find_any(xor_nested_strategy(), |&x| x == 3);
}

#[test]
fn test_xor_flattens_branches_4() {
    find_any(xor_nested_strategy(), |&x| x == 4);
}

#[test]
fn test_xor_flattens_branches_5() {
    find_any(xor_nested_strategy(), |&x| x == 5);
}

#[test]
fn test_xor_flattens_branches_6() {
    find_any(xor_nested_strategy(), |&x| x == 6);
}

#[test]
fn test_xor_flattens_branches_7() {
    find_any(xor_nested_strategy(), |&x| x == 7);
}

// one_of_nested_strategy_with_map produces {1, 4, 6, 16, 20, 24, 28, 32}.
fn one_of_nested_strategy_with_map() -> impl Generator<i64> {
    gs::one_of(vec![
        gs::just(1i64).boxed(),
        gs::one_of(vec![
            // (just(2) | just(3)).map(double) → 4 or 6
            gs::one_of(vec![gs::just(2i64).boxed(), gs::just(3i64).boxed()])
                .map(|x| x * 2)
                .boxed(),
            // one_of(...).map(double) → 16, 20, 24, 28, or 32
            gs::one_of(vec![
                // (just(4) | just(5)).map(double) → 8 or 10
                gs::one_of(vec![gs::just(4i64).boxed(), gs::just(5i64).boxed()])
                    .map(|x| x * 2)
                    .boxed(),
                // one_of((just(6)|just(7)|just(8)).map(double)) → 12, 14, or 16
                gs::one_of(vec![
                    gs::one_of(vec![
                        gs::just(6i64).boxed(),
                        gs::just(7i64).boxed(),
                        gs::just(8i64).boxed(),
                    ])
                    .map(|x| x * 2)
                    .boxed(),
                ])
                .boxed(),
            ])
            .map(|x| x * 2)
            .boxed(),
        ])
        .boxed(),
    ])
}

#[test]
fn test_one_of_flattens_map_branches_1() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 1);
}

#[test]
fn test_one_of_flattens_map_branches_4() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 4);
}

#[test]
fn test_one_of_flattens_map_branches_6() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 6);
}

#[test]
fn test_one_of_flattens_map_branches_16() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 16);
}

#[test]
fn test_one_of_flattens_map_branches_20() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 20);
}

#[test]
fn test_one_of_flattens_map_branches_24() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 24);
}

#[test]
fn test_one_of_flattens_map_branches_28() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 28);
}

#[test]
fn test_one_of_flattens_map_branches_32() {
    find_any(one_of_nested_strategy_with_map(), |&x| x == 32);
}

// one_of_nested_strategy_with_flatmap: just(None).flatmap(lambda x: one_of(…))
// produces Vec<()> of length 0..7.
fn one_of_nested_strategy_with_flatmap() -> impl Generator<Vec<()>> {
    gs::just(()).flat_map(|_| {
        gs::one_of(vec![
            gs::just(vec![(); 0]).boxed(),
            gs::just(vec![(); 1]).boxed(),
            gs::one_of(vec![
                gs::just(vec![(); 2]).boxed(),
                gs::just(vec![(); 3]).boxed(),
                gs::one_of(vec![
                    gs::just(vec![(); 4]).boxed(),
                    gs::just(vec![(); 5]).boxed(),
                    gs::one_of(vec![
                        gs::just(vec![(); 6]).boxed(),
                        gs::just(vec![(); 7]).boxed(),
                    ])
                    .boxed(),
                ])
                .boxed(),
            ])
            .boxed(),
        ])
    })
}

#[test]
fn test_one_of_flattens_flatmap_branches_0() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.is_empty()
    });
}

#[test]
fn test_one_of_flattens_flatmap_branches_1() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 1
    });
}

#[test]
fn test_one_of_flattens_flatmap_branches_2() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 2
    });
}

#[test]
fn test_one_of_flattens_flatmap_branches_3() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 3
    });
}

#[test]
fn test_one_of_flattens_flatmap_branches_4() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 4
    });
}

#[test]
fn test_one_of_flattens_flatmap_branches_5() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 5
    });
}

#[test]
fn test_one_of_flattens_flatmap_branches_6() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 6
    });
}

#[test]
fn test_one_of_flattens_flatmap_branches_7() {
    find_any(one_of_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 7
    });
}

// xor_nested_strategy_with_flatmap: same values, different | nesting.
fn xor_nested_strategy_with_flatmap() -> impl Generator<Vec<()>> {
    gs::just(()).flat_map(|_| {
        gs::one_of(vec![
            gs::just(vec![(); 0]).boxed(),
            gs::one_of(vec![
                gs::just(vec![(); 1]).boxed(),
                gs::one_of(vec![
                    gs::one_of(vec![
                        gs::just(vec![(); 2]).boxed(),
                        gs::just(vec![(); 3]).boxed(),
                    ])
                    .boxed(),
                    gs::one_of(vec![
                        gs::one_of(vec![
                            gs::just(vec![(); 4]).boxed(),
                            gs::just(vec![(); 5]).boxed(),
                        ])
                        .boxed(),
                        gs::one_of(vec![
                            gs::just(vec![(); 6]).boxed(),
                            gs::just(vec![(); 7]).boxed(),
                        ])
                        .boxed(),
                    ])
                    .boxed(),
                ])
                .boxed(),
            ])
            .boxed(),
        ])
    })
}

#[test]
fn test_xor_flattens_flatmap_branches_0() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.is_empty()
    });
}

#[test]
fn test_xor_flattens_flatmap_branches_1() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 1
    });
}

#[test]
fn test_xor_flattens_flatmap_branches_2() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 2
    });
}

#[test]
fn test_xor_flattens_flatmap_branches_3() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 3
    });
}

#[test]
fn test_xor_flattens_flatmap_branches_4() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 4
    });
}

#[test]
fn test_xor_flattens_flatmap_branches_5() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 5
    });
}

#[test]
fn test_xor_flattens_flatmap_branches_6() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 6
    });
}

#[test]
fn test_xor_flattens_flatmap_branches_7() {
    find_any(xor_nested_strategy_with_flatmap(), |x: &Vec<()>| {
        x.len() == 7
    });
}

// one_of_nested_strategy_with_filter: produces even values {0, 2, 4, 6}.
fn one_of_nested_strategy_with_filter() -> impl Generator<i64> {
    gs::one_of(vec![
        gs::just(0i64).boxed(),
        gs::just(1i64).boxed(),
        gs::one_of(vec![
            gs::just(2i64).boxed(),
            gs::just(3i64).boxed(),
            gs::one_of(vec![
                gs::just(4i64).boxed(),
                gs::just(5i64).boxed(),
                gs::one_of(vec![gs::just(6i64).boxed(), gs::just(7i64).boxed()]).boxed(),
            ])
            .boxed(),
        ])
        .boxed(),
    ])
    .filter(|x: &i64| x % 2 == 0)
}

#[test]
fn test_one_of_flattens_filter_branches_0() {
    find_any(one_of_nested_strategy_with_filter(), |&x| x == 0);
}

#[test]
fn test_one_of_flattens_filter_branches_1() {
    find_any(one_of_nested_strategy_with_filter(), |&x| x == 2);
}

#[test]
fn test_one_of_flattens_filter_branches_2() {
    find_any(one_of_nested_strategy_with_filter(), |&x| x == 4);
}

#[test]
fn test_one_of_flattens_filter_branches_3() {
    find_any(one_of_nested_strategy_with_filter(), |&x| x == 6);
}

#[test]
fn test_long_duplicates_strings() {
    find_any(
        gs::tuples!(gs::text(), gs::text()),
        |(s0, s1): &(String, String)| s0.len() >= 5 && s0 == s1,
    );
}

#[test]
fn test_can_produce_nasty_strings() {
    let nasty: HashSet<&str> = ["NaN", "Inf", "undefined"].into_iter().collect();
    find_any(gs::text(), move |s: &String| nasty.contains(s.as_str()));
}
