use crate::common::utils::{Minimal, minimal};
use hegel::generators::{self as gs, Generator, PrintableGenerator};

fn int_pair(lo: i64, hi: i64) -> impl Generator<(i64, i64)> {
    hegel::compose!(|tc| {
        let a: i64 = tc.draw(gs::integers::<i64>().min_value(lo).max_value(hi));
        let b: i64 = tc.draw(gs::integers::<i64>().min_value(lo).max_value(hi));
        (a, b)
    })
}

#[test]
fn test_positive_sum_of_pair() {
    let (a, b) = minimal(int_pair(0, 1000), |(a, b): &(i64, i64)| *a + *b > 1000);
    assert_eq!((a, b), (1, 1000));
}

#[test]
fn test_negative_sum_of_pair() {
    let (a, b) = minimal(int_pair(-1000, 1000), |(a, b): &(i64, i64)| *a + *b < -1000);
    assert_eq!((a, b), (-1, -1000));
}

#[test]
fn test_sum_of_pair_separated() {
    let separated_sum = hegel::compose!(|tc| {
        let n1: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
        let _: String = tc.draw(gs::text());
        let _: bool = tc.draw(gs::booleans());
        let _: i64 = tc.draw(gs::integers::<i64>());
        let n2: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1000));
        (n1, n2)
    });
    let (n1, n2) = minimal(separated_sum, |(a, b): &(i64, i64)| *a + *b > 1000);
    assert_eq!((n1, n2), (1, 1000));
}

#[test]
fn test_minimize_dict_of_booleans() {
    let (a, b) = minimal(
        gs::tuples!(gs::booleans(), gs::booleans()),
        |(a, b): &(bool, bool)| *a || *b,
    );
    assert!(!(a && b));
    assert!(a || b);
}

#[test]
fn test_minimize_namedtuple() {
    let int_struct = hegel::compose!(|tc| {
        let a: i64 = tc.draw(gs::integers::<i64>());
        let b: i64 = tc.draw(gs::integers::<i64>());
        (a, b)
    });
    let (a, b) = minimal(int_struct, |(a, b): &(i64, i64)| *a < *b);
    assert_eq!(b, a + 1);
}

#[test]
fn test_earlier_exit_produces_shorter_sequence() {
    let g = hegel::compose!(|tc| {
        let v0: bool = tc.draw(gs::booleans());
        let _v1 = (tc.draw(gs::booleans()), tc.draw(gs::booleans()));
        let _v2 = (tc.draw(gs::booleans()), tc.draw(gs::booleans()));
        if !v0 {
            let _: bool = tc.draw(gs::booleans());
        }
        v0
    });
    let v0 = minimal(g, |_: &bool| true);
    assert!(v0, "shrinker should prefer the shorter v0=true path");
}

#[derive(Debug, Clone, PartialEq, hegel::PrettyPrintable)]
enum BoolOrFloat {
    Bool(bool),
    Float(f64),
}

#[test]
fn test_one_of_shrinks_branch_selector() {
    let result = minimal(
        gs::one_of(vec![
            gs::booleans().map(BoolOrFloat::Bool).boxed_printable(),
            gs::floats::<f64>()
                .allow_nan(false)
                .allow_infinity(false)
                .map(BoolOrFloat::Float)
                .boxed_printable(),
        ]),
        |v: &BoolOrFloat| match v {
            BoolOrFloat::Bool(b) => *b,
            BoolOrFloat::Float(f) => *f != 0.0,
        },
    );
    assert_eq!(result, BoolOrFloat::Bool(true));
}

#[test]
fn test_early_exit_via_flag_with_preceding_draws() {
    let g = hegel::compose!(|tc| {
        let v0: bool = tc.draw(gs::booleans());
        let v1: Vec<u8> = tc.draw(gs::binary().max_size(20));
        let v2: Vec<i64> =
            tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(0)).max_size(10));
        (v0, v1, v2)
    });
    let (v0, v1, v2) = minimal(g, |(v0, v1, v2): &(bool, Vec<u8>, Vec<i64>)| {
        *v0 || v1.len() + v2.len() >= 20
    });
    let _ = (v0, v1, v2);
}

#[test]
fn test_one_of_branch_switch_with_trailing_draws() {
    let test_data = hegel::compose!(|tc| {
        let v0 = tc.draw(gs::one_of(vec![
            gs::booleans().map(BoolOrFloat::Bool).boxed_printable(),
            gs::floats::<f64>()
                .allow_nan(false)
                .allow_infinity(false)
                .map(BoolOrFloat::Float)
                .boxed_printable(),
        ]));
        let _: (bool, bool) = tc.draw(hegel::compose!(|tc| {
            (tc.draw(gs::booleans()), tc.draw(gs::booleans()))
        }));
        v0
    });
    let result = minimal(test_data, |v: &BoolOrFloat| match v {
        BoolOrFloat::Bool(b) => *b,
        BoolOrFloat::Float(f) => *f != 0.0,
    });
    assert_eq!(result, BoolOrFloat::Bool(true));
}

#[test]
fn test_shorter_path_via_later_assertion() {
    let pair = || {
        hegel::compose!(|tc| {
            let a: bool = tc.draw(gs::booleans());
            let b: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
            (a, b)
        })
    };
    let test_data = hegel::compose!(|tc| {
        let v0: (bool, f64) = tc.draw(pair());
        let v1: Vec<i64> = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(20))
                .max_size(10)
                .unique(true),
        );
        let _: (bool, f64) = tc.draw(pair());
        (v0, v1)
    });
    let (_v0, v1) = minimal(test_data, |_: &((bool, f64), Vec<i64>)| true);
    assert!(v1.is_empty());
}

#[test]
fn test_one_of_branch_switch_to_float() {
    let result = minimal(
        gs::one_of(vec![
            gs::floats::<f64>()
                .allow_nan(false)
                .allow_infinity(false)
                .map(BoolOrFloat::Float)
                .boxed_printable(),
            gs::booleans().map(BoolOrFloat::Bool).boxed_printable(),
        ]),
        |_: &BoolOrFloat| true,
    );
    assert_eq!(result, BoolOrFloat::Float(0.0));
}

#[derive(Debug, Clone, PartialEq, hegel::PrettyPrintable)]
enum TupOrBool {
    Tup((bool, bool)),
    Bool(bool),
}

#[test]
fn test_one_of_shorter_branch_needs_non_simplest_value() {
    let result = minimal(
        gs::one_of(vec![
            gs::tuples!(gs::booleans(), gs::booleans())
                .map(TupOrBool::Tup)
                .boxed_printable(),
            gs::booleans().map(TupOrBool::Bool).boxed_printable(),
        ]),
        |v: &TupOrBool| match v {
            TupOrBool::Tup((a, b)) => *a || *b,
            TupOrBool::Bool(b) => *b,
        },
    );
    assert_eq!(result, TupOrBool::Bool(true));
}

#[test]
fn test_switch_failure_mode_for_simpler_sort_key() {
    let test_data = hegel::compose!(|tc| {
        let v1: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
        let v4: i64 = tc.draw(gs::sampled_from(vec![1i64, 0]));
        (v1, v4)
    });
    let (v1, _v4) = minimal(test_data, |(v1, v4): &(f64, i64)| {
        v1.abs() >= 1.0 || *v4 > 0
    });
    assert_eq!(v1, 0.0);
}

#[test]
fn test_shorter_path_when_guard_precedes_expensive_draw() {
    let test_data = hegel::compose!(|tc| {
        let v0: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
        let v1: bool = tc.draw(gs::booleans());
        let v2: Vec<i64> =
            tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)).max_size(10));
        (v0, v1, v2)
    });
    let (v0, _v1, _v2) = minimal(test_data, |(v0, _, v2): &(i64, bool, Vec<i64>)| {
        *v0 > 0 || v2.len() >= 3
    });
    assert!(v0 > 0);
}

#[test]
fn test_finds_small_list_even_with_bad_lists() {
    let bad_list = hegel::compose!(|tc| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
        (0..n)
            .map(|_| tc.draw(gs::integers::<i64>().min_value(0).max_value(10000)))
            .collect::<Vec<i64>>()
    });
    let result = Minimal::new(bad_list, |ls: &Vec<i64>| ls.iter().sum::<i64>() > 1000)
        .test_cases(2000)
        .run();
    assert_eq!(result, vec![1001]);
}

#[test]
fn test_shrinking_mixed_choice_types_no_sort_crash() {
    let g = hegel::compose!(|tc| {
        let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(3));
        if x > 0 {
            let _: bool = tc.draw(gs::booleans());
            let _: bool = tc.draw(gs::booleans());
        }
        x
    });
    assert_eq!(minimal(g, |_: &i64| true), 0);
}

#[test]
fn test_shrinking_stale_indices_no_redistribute_crash() {
    let g = hegel::compose!(|tc| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(2).max_value(8));
        let vals: Vec<i64> = (0..n)
            .map(|_| tc.draw(gs::integers::<i64>().min_value(0).max_value(100)))
            .collect();
        let _: bool = tc.draw(gs::booleans());
        vals
    });
    let vals = Minimal::new(g, |vals: &Vec<i64>| {
        vals.iter().sum::<i64>() > 150 && vals.len() >= 3
    })
    .test_cases(2000)
    .run();
    assert_eq!(vals, vec![0, 51, 100]);
}

#[derive(Debug, Clone, PartialEq, hegel::PrettyPrintable)]
enum BoolOrInt {
    Bool(bool),
    Int(i64),
}

#[test]
fn test_lower_and_bump_with_type_change() {
    let result = minimal(
        gs::one_of(vec![
            gs::booleans().map(BoolOrInt::Bool).boxed_printable(),
            gs::integers::<i64>()
                .min_value(0)
                .max_value(100)
                .map(BoolOrInt::Int)
                .boxed_printable(),
        ]),
        |v: &BoolOrInt| matches!(v, BoolOrInt::Int(n) if *n > 50),
    );
    assert_eq!(result, BoolOrInt::Int(51));
}

#[test]
fn test_lower_and_bump_explores_new_range() {
    let g = hegel::compose!(|tc| {
        let v0: i64 = tc.draw(gs::sampled_from(vec![32i64, 46]));
        let v1: i64 = tc.draw(gs::sampled_from(vec![32i64, 46]));
        let v2: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(-(v0.saturating_abs() + 1))
                .max_value(v0.saturating_abs() + 1),
        );
        let v3: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(-(v2.saturating_abs() + 1))
                .max_value(v2.saturating_abs() + 1),
        );
        (v0, v1, v2, v3)
    });
    let (v0, v1, v2, v3) = Minimal::new(g, |(v0, _, v2, _): &(i64, i64, i64, i64)| v2 == v0)
        .test_cases(2000)
        .run();
    assert_eq!(v2, v0);
    assert_eq!(v1, 32);
    assert_eq!(v3, 0);
    assert_eq!(v0, 32);
}

#[test]
fn test_lower_and_bump_tries_negative_values() {
    let pair = || {
        hegel::compose!(|tc| {
            let a: bool = tc.draw(gs::booleans());
            let b: bool = tc.draw(gs::booleans());
            (a, b)
        })
    };
    let g = hegel::compose!(|tc| {
        let _v0 = tc.draw(pair());
        let _v1 = tc.draw(pair());
        let v2 = tc.draw(gs::one_of(vec![
            gs::integers::<i64>()
                .min_value(0)
                .max_value(0)
                .map(BoolOrInt::Int)
                .boxed_printable(),
            gs::booleans().map(BoolOrInt::Bool).boxed_printable(),
        ]));
        let v3: i64 = tc.draw(gs::integers::<i64>().min_value(-1).max_value(1));
        (v2, v3)
    });
    let (v2, v3) = Minimal::new(g, |(v2, v3): &(BoolOrInt, i64)| {
        matches!(v2, BoolOrInt::Bool(true)) || (matches!(v2, BoolOrInt::Bool(false)) && *v3 < 0)
    })
    .test_cases(2000)
    .run();
    match v2 {
        BoolOrInt::Bool(true) => assert_eq!(v3, 0),
        BoolOrInt::Bool(false) => assert_eq!(v3, -1),
        BoolOrInt::Int(_) => panic!("unexpected int branch"),
    }
}

#[test]
fn test_increment_to_max_shortens_via_sampled_from() {
    let g = hegel::compose!(|tc| {
        let v0: i64 = tc.draw(gs::sampled_from(vec![1i64, 1, 0]));
        if v0 <= 0 {
            return (v0, None);
        }
        let v1: bool = tc.draw(gs::booleans());
        (v0, Some(v1))
    });
    let (v0, v1) = Minimal::new(g, |(v0, v1): &(i64, Option<bool>)| {
        *v0 <= 0 || v1.is_some_and(|b| b)
    })
    .test_cases(2000)
    .run();
    assert_eq!(v0, 0);
    assert!(v1.is_none());
}

#[test]
fn test_lower_and_bump_targets_booleans() {
    let g = hegel::compose!(|tc| {
        let v0: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1));
        let v1: bool = tc.draw(gs::booleans());
        (v0, v1)
    });
    assert_eq!(minimal(g, |_: &(i64, bool)| true), (0, false));
}

#[test]
fn test_increment_with_dependent_continuation() {
    let g = hegel::compose!(|tc| {
        let v0: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(0));
        let v1: bool = tc.draw(gs::booleans());
        let _: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(0));
        let v3: Vec<i64> = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(-21).max_value(-1))
                .max_size(10)
                .unique(true),
        );
        let v4: Option<i64> = if v1 {
            Some(tc.draw(gs::integers::<i64>().min_value(v0).max_value(v0 + 1)))
        } else {
            None
        };
        (v0, v1, v3, v4)
    });
    let (_v0, v1, v3, _v4) = Minimal::new(
        g,
        |(v0, v1, v3, v4): &(i64, bool, Vec<i64>, Option<i64>)| {
            !v3.is_empty() || (*v1 && v4.is_some_and(|v| *v0 + v <= 0))
        },
    )
    .test_cases(2000)
    .run();
    assert!(v3.is_empty());
    assert!(v1);
}

#[test]
fn test_lower_and_bump_with_float_target() {
    let g = hegel::compose!(|tc| {
        let v0: String = tc.draw(gs::text().min_codepoint(32).max_codepoint(126).max_size(20));
        let v1: f64 = tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false));
        (v0, v1)
    });
    let (v0, _v1) = Minimal::new(g, |(v0, v1): &(String, f64)| {
        v0.chars().count() >= 4 || *v1 != 0.0
    })
    .test_cases(2000)
    .run();
    assert_eq!(v0, "");
}

#[derive(Debug, Clone, PartialEq, hegel::PrettyPrintable)]
enum BoolIntOrInt {
    Bool(bool),
    Z,
    Two,
}

#[test]
fn test_redistribute_stale_indices_with_one_of() {
    let g = hegel::compose!(|tc| {
        let v0 = tc.draw(gs::one_of(vec![
            gs::booleans().map(BoolIntOrInt::Bool).boxed_printable(),
            gs::integers::<i64>()
                .min_value(0)
                .max_value(0)
                .map(|_| BoolIntOrInt::Z)
                .boxed_printable(),
            gs::integers::<i64>()
                .min_value(2)
                .max_value(2)
                .filter(|x: &i64| *x > 0)
                .map(|_| BoolIntOrInt::Two)
                .boxed_printable(),
        ]));
        let _: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(0));
        v0
    });
    let v = Minimal::new(g, |v: &BoolIntOrInt| {
        matches!(v, BoolIntOrInt::Bool(true) | BoolIntOrInt::Two)
    })
    .test_cases(2000)
    .run();
    assert_eq!(v, BoolIntOrInt::Bool(true));
}

#[test]
fn test_lower_and_bump_stale_j_after_replace() {
    let g = hegel::compose!(|tc| {
        let v0: bool = tc.draw(gs::booleans());
        let _: bool = tc.draw(gs::booleans());
        let _: bool = tc.draw(gs::booleans());
        let _: Vec<i64> = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(0))
                .max_size(10)
                .filter(|x: &Vec<i64>| !x.is_empty()),
        );
        let _: Vec<i64> = tc.draw(
            gs::integers::<i64>()
                .min_value(-54)
                .max_value(-32)
                .flat_map(|n| {
                    let size = (n.unsigned_abs() as usize) % 5;
                    gs::vecs(gs::integers::<i64>().min_value(0).max_value(100))
                        .min_size(size)
                        .max_size(size + 1)
                }),
        );
        v0
    });
    let v = Minimal::new(g, |v: &bool| *v).test_cases(2000).run();
    assert!(v);
}

#[test]
fn test_mutation_with_single_value_adjacent() {
    let g = hegel::compose!(|tc| {
        let v0: bool = tc.draw(gs::booleans());
        let _: i64 = tc.draw(gs::integers::<i64>().min_value(5).max_value(5));
        v0
    });
    let v0 = minimal(g, |v: &bool| *v);
    assert!(v0);
}

#[test]
fn test_shrink_duplicates_two_copies() {
    let (a, b) = Minimal::new(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(100),
            gs::integers::<i64>().min_value(0).max_value(100),
        ),
        |(a, b): &(i64, i64)| *a == *b && *a > 0,
    )
    .test_cases(10000)
    .run();
    assert_eq!(a, 1);
    assert_eq!(b, 1);
}

#[test]
fn test_shrink_duplicates_three_copies() {
    let (a, b, c) = Minimal::new(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(10),
            gs::integers::<i64>().min_value(0).max_value(10),
            gs::integers::<i64>().min_value(0).max_value(10),
        ),
        |(a, b, c): &(i64, i64, i64)| *a == *b && *b == *c && *a > 0,
    )
    .test_cases(10000)
    .run();
    assert_eq!(a, 1);
    assert_eq!(b, 1);
    assert_eq!(c, 1);
}

#[derive(Debug, Clone, PartialEq, hegel::PrettyPrintable)]
enum ListOrIntOrBool {
    List(Vec<i64>),
    Zero,
    Bool(bool),
}

#[test]
fn test_one_of_switches_to_shorter_branch() {
    let inner = || {
        gs::one_of(vec![
            gs::integers::<i64>()
                .min_value(0)
                .max_value(0)
                .map(|_| ListOrIntOrBool::Zero)
                .boxed_printable(),
            gs::booleans().map(ListOrIntOrBool::Bool).boxed_printable(),
        ])
    };
    let outer = gs::one_of(vec![
        gs::vecs(gs::integers::<i64>().min_value(0).max_value(0))
            .max_size(10)
            .map(ListOrIntOrBool::List)
            .boxed_printable(),
        inner().boxed_printable(),
    ]);
    let result = minimal(outer, |v: &ListOrIntOrBool| match v {
        ListOrIntOrBool::List(xs) => !xs.is_empty(),
        ListOrIntOrBool::Zero => false,
        ListOrIntOrBool::Bool(b) => *b,
    });
    assert_eq!(result, ListOrIntOrBool::Bool(true));
}

#[test]
fn test_mutate_exercises_index_probes() {
    let (a, b) = minimal(
        gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(10),
            gs::booleans(),
        ),
        |(a, b): &(i64, bool)| *a > 5 && *b,
    );
    assert!(a > 5);
    assert!(b);
}

#[test]
fn test_mutate_skips_large_result() {
    let g = hegel::compose!(|tc| {
        (0..35)
            .map(|_| tc.draw(gs::integers::<i64>().min_value(0).max_value(10)))
            .collect::<Vec<i64>>()
    });
    let v = Minimal::new(g, |_: &Vec<i64>| true).test_cases(200).run();
    assert_eq!(v, vec![0i64; 35]);
}

/// Five integers; the predicate accepts iff all non-zero values appear
/// before any zero. The shrinker should compact to all zeroes (the
/// predicate's "everything zero" branch).
#[test]
fn test_can_expand_zeroed_region() {
    let g = hegel::compose!(|tc| {
        let mut nums: Vec<i64> = Vec::new();
        for _ in 0..5 {
            nums.push(tc.draw(gs::integers::<i64>().min_value(0).max_value(255)));
        }
        nums
    });
    let v = Minimal::new(g, |nums: &Vec<i64>| {
        let mut seen_zero = false;
        for &n in nums {
            if n == 0 {
                seen_zero = true;
            } else if seen_zero {
                return false;
            }
        }
        true
    })
    .test_cases(5000)
    .run();
    assert_eq!(v, vec![0, 0, 0, 0, 0]);
}

/// The shrinker should preserve the trailing 0 sentinel while reducing
/// the middle-of-sequence to minimal contents that still trigger the
/// 6-marker — converging on a sequence whose simplest form ends with
/// the 6 and the terminator.
#[test]
fn test_retain_end_of_buffer() {
    let g = hegel::compose!(|tc| {
        let mut nums: Vec<i64> = Vec::new();
        loop {
            let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(255));
            nums.push(n);
            if n == 0 {
                break;
            }
            if nums.len() > 50 {
                break;
            }
        }
        nums
    });
    let v = Minimal::new(g, |nums: &Vec<i64>| nums.contains(&6))
        .test_cases(5000)
        .run();
    assert!(v.contains(&6));
    assert!(
        v.len() <= 3,
        "expected short tail-preserving result, got {:?}",
        v
    );
}

/// Two equal positive integers x, y followed by `x & 255` bytes each of
/// value 1. Predicate: x == y AND the bytes set has at most one element.
/// Minimum: (0, 0) with no trailing bytes — both x and y can shrink to
/// zero together via `minimize_duplicated_choices`.
#[test]
fn test_duplicate_nodes_that_go_away() {
    let g = hegel::compose!(|tc| {
        let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(255));
        let y: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(255));
        let mut bytes: Vec<i64> = Vec::new();
        for _ in 0..(x & 255) {
            bytes.push(tc.draw(gs::integers::<i64>().min_value(0).max_value(255)));
        }
        (x, y, bytes)
    });
    let (x, y, b) = Minimal::new(g, |(x, y, b): &(i64, i64, Vec<i64>)| {
        if x != y {
            return false;
        }
        let set: std::collections::HashSet<&i64> = b.iter().collect();
        set.len() <= 1
    })
    .test_cases(5000)
    .run();
    assert_eq!((x, y), (0, 0));
    assert!(b.is_empty());
}
