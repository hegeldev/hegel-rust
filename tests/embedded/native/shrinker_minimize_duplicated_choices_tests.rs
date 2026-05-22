//! Unit tests for the generalised `shrink_duplicates` /
//! `minimize_duplicated_choices` (Step 7).
//!
//! Hypothesis reference: `shrinker.py:1379-1406`.

use crate::native::core::choices::{
    BooleanChoice, BytesChoice, FloatChoice, IntegerChoice, StringChoice,
};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(value),
        was_forced: false,
    }
}

fn float_node(value: f64) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: false,
        }),
        value: ChoiceValue::Float(value),
        was_forced: false,
    }
}

fn bytes_node(value: Vec<u8>) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Bytes(BytesChoice {
            min_size: 0,
            max_size: 16,
        }),
        value: ChoiceValue::Bytes(value),
        was_forced: false,
    }
}

fn integer_node(value: i128, min_value: i128, max_value: i128) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Integer(IntegerChoice {
            min_value,
            max_value,
            shrink_towards: 0,
        }),
        value: ChoiceValue::Integer(value),
        was_forced: false,
    }
}

fn string_node(value: Vec<u32>) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(0, 0x10FFFF)]),
            min_size: 0,
            max_size: 16,
        }),
        value: ChoiceValue::String(value),
        was_forced: false,
    }
}

fn accepting_shrinker(initial: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    )
}

#[test]
fn shrink_duplicates_collapses_paired_booleans_to_false() {
    let mut shrinker = accepting_shrinker(vec![bool_node(true), bool_node(true)]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Boolean(b) => assert!(!b),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_collapses_paired_floats_to_zero() {
    let mut shrinker = accepting_shrinker(vec![float_node(3.5), float_node(3.5)]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Float(v) => assert_eq!(v, 0.0),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_collapses_paired_bytes_to_empty() {
    let mut shrinker =
        accepting_shrinker(vec![bytes_node(vec![1, 2, 3]), bytes_node(vec![1, 2, 3])]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match &n.value {
            ChoiceValue::Bytes(b) => assert!(b.is_empty()),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_collapses_paired_strings_to_empty() {
    let mut shrinker = accepting_shrinker(vec![
        string_node(vec![b'a' as u32, b'b' as u32]),
        string_node(vec![b'a' as u32, b'b' as u32]),
    ]);
    shrinker.shrink_duplicates();
    for n in &shrinker.current_nodes {
        match &n.value {
            ChoiceValue::String(s) => assert!(s.is_empty()),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_leaves_solo_nodes_alone() {
    // Single non-duplicate of each kind — the generalised pass shouldn't
    // change them.  Predicate accepts everything; only the simplest-step
    // could fire, but each group has only one member.
    let mut shrinker =
        accepting_shrinker(vec![bool_node(true), float_node(3.0), bytes_node(vec![5])]);
    shrinker.shrink_duplicates();
    match shrinker.current_nodes[0].value {
        ChoiceValue::Boolean(b) => assert!(b),
        _ => unreachable!(),
    }
    match shrinker.current_nodes[1].value {
        ChoiceValue::Float(v) => assert_eq!(v, 3.0),
        _ => unreachable!(),
    }
    match &shrinker.current_nodes[2].value {
        ChoiceValue::Bytes(b) => assert_eq!(b, &vec![5]),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_duplicates_keeps_distinct_values_separate() {
    // Three booleans, only two of them duplicates.  The duplicates
    // should be lowered together; the third value should be left alone.
    let mut shrinker = accepting_shrinker(vec![bool_node(true), bool_node(false), bool_node(true)]);
    shrinker.shrink_duplicates();
    // After shrink: the two trues went to false, the original false
    // stayed.  Result: [false, false, false].
    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Boolean(b) => assert!(!b),
            _ => unreachable!(),
        }
    }
}

/// Predicate "all members of the integer group are equal and >=
/// threshold" — interesting iff they form a uniform value at or above
/// threshold.  Drives `shrink_duplicates` into its multi-member
/// descent path (L580 in `integers.rs`) so we can assert the
/// shift_right descent uses ~log log instead of ~log accept_improvements.
fn group_accepts_uniform_at_least(initial: Vec<ChoiceNode>, threshold: i128) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let int_vals: Vec<i128> = nodes
                    .iter()
                    .filter_map(|n| match &n.value {
                        ChoiceValue::Integer(v) => Some(*v),
                        _ => None,
                    })
                    .collect();
                let interesting = int_vals.len() >= 2
                    && int_vals.iter().all(|v| *v == int_vals[0])
                    && int_vals[0] >= threshold;
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    )
}

#[test]
fn shrink_duplicates_positive_descent_is_log_log() {
    // Five copies of a very large integer (10^15) constrained to
    // remain equal and >= 100.  With shift_right descent this should
    // converge in ~7 accept_improvements (log log of 10^15);
    // bin_search_down would take ~50.
    let initial: Vec<ChoiceNode> = (0..5)
        .map(|_| integer_node(1_000_000_000_000_000, 0, i128::MAX))
        .collect();
    let mut shrinker = group_accepts_uniform_at_least(initial, 100);
    shrinker.shrink_duplicates();

    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Integer(v) => assert_eq!(v, 100),
            _ => unreachable!(),
        }
    }
    assert!(
        shrinker.improvements < 30,
        "shrink_duplicates positive descent should be log-log; saw {} improvements",
        shrinker.improvements
    );
}

#[test]
fn shrink_duplicates_negative_descent_is_log_log() {
    // Mirror of the positive case: predicate accepts uniform-negative
    // groups <= -threshold.  Tests the L602 bin_search_down branch.
    let initial: Vec<ChoiceNode> = (0..5)
        .map(|_| integer_node(-1_000_000_000_000_000, i128::MIN + 1, 0))
        .collect();
    // Predicate accepts uniform integer groups <= -100.
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let int_vals: Vec<i128> = nodes
                    .iter()
                    .filter_map(|n| match &n.value {
                        ChoiceValue::Integer(v) => Some(*v),
                        _ => None,
                    })
                    .collect();
                let interesting = int_vals.len() >= 2
                    && int_vals.iter().all(|v| *v == int_vals[0])
                    && int_vals[0] <= -100;
                (interesting, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_duplicates();

    for n in &shrinker.current_nodes {
        match n.value {
            ChoiceValue::Integer(v) => assert_eq!(v, -100),
            _ => unreachable!(),
        }
    }
    assert!(
        shrinker.improvements < 30,
        "shrink_duplicates negative descent should be log-log; saw {} improvements",
        shrinker.improvements
    );
}

/// Cover the `valid.len() < 2` continue at integers.rs:573-575 inside
/// `shrink_duplicates`.  Two groups (value=7 and value=8); the
/// test_fn collapses every position to 0 on every Full call so
/// accept_improvement fires whichever group is processed first
/// (HashMap iteration is non-deterministic).  After the first
/// group's replace lands, the OTHER group's re-validation finds zero
/// matching members and hits the branch.
#[test]
fn shrink_duplicates_skips_group_invalidated_by_concurrent_shrink() {
    let initial = vec![
        integer_node(7, 0, i128::MAX),
        integer_node(7, 0, i128::MAX),
        integer_node(8, 0, i128::MAX),
        integer_node(8, 0, i128::MAX),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let len = nodes.len();
                let zeros: Vec<ChoiceNode> =
                    (0..len).map(|_| integer_node(0, 0, i128::MAX)).collect();
                (true, zeros, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_duplicates();
}

/// Cover the `current_valid.len() < 2` return inside the
/// `group_replace` closure (integers.rs:613-615).  Predicate accepts
/// only positive nodes[0] and truncates the realised sequence to a
/// single element.  The simplest step (set all to 0) is rejected;
/// the find_integer shift_right probe lands a positive candidate that
/// accept_improvement absorbs; the NEXT probe inside the same
/// find_integer loop calls group_replace, whose `current_valid`
/// filter now finds only one member in range and short-circuits.
#[test]
fn shrink_duplicates_group_replace_short_circuits_when_truncated() {
    let initial: Vec<ChoiceNode> = (0..5)
        .map(|_| integer_node(1_000_000_000_000_000, 0, i128::MAX))
        .collect();
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let n = match nodes[0].value {
                    ChoiceValue::Integer(v) => v,
                    _ => 0,
                };
                if n <= 0 {
                    return (false, nodes.to_vec(), Spans::new());
                }
                // Accept and truncate the realised sequence to one
                // node so the next group_replace probe finds < 2
                // valid members.
                (true, vec![nodes[0].clone()], Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_duplicates();
}
