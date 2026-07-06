//! Unit tests for the generalised `shrink_duplicates` /
//! `minimize_duplicated_choices`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::{
    BooleanChoice, BytesChoice, FloatChoice, IntegerChoice, StringChoice,
};
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

fn float_node(value: f64) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Float(FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: false,
            allow_infinity: false,
            smallest_nonzero_magnitude: 5e-324,
        }),
        ChoiceValue::Float(value),
        false,
    )
}

fn bytes_node(value: Vec<u8>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Bytes(BytesChoice {
            min_size: 0,
            max_size: 16,
        }),
        ChoiceValue::Bytes(value),
        false,
    )
}

fn integer_node(value: i128, min_value: i128, max_value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(min_value),
            max_value: BigInt::from(max_value),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn string_node(value: Vec<u32>) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::String(StringChoice {
            intervals: IntervalSet::new(vec![(0, 0x10FFFF)]).into(),
            min_size: 0,
            max_size: 16,
        }),
        ChoiceValue::String(value),
        false,
    )
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
    shrinker.shrink_duplicates().unwrap();
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
    shrinker.shrink_duplicates().unwrap();
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
    shrinker.shrink_duplicates().unwrap();
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
    shrinker.shrink_duplicates().unwrap();
    for n in &shrinker.current_nodes {
        match &n.value {
            ChoiceValue::String(s) => assert!(s.is_empty()),
            _ => unreachable!(),
        }
    }
}

#[test]
fn shrink_duplicates_leaves_solo_nodes_alone() {
    let mut shrinker =
        accepting_shrinker(vec![bool_node(true), float_node(3.0), bytes_node(vec![5])]);
    shrinker.shrink_duplicates().unwrap();
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
    let mut shrinker = accepting_shrinker(vec![bool_node(true), bool_node(false), bool_node(true)]);
    shrinker.shrink_duplicates().unwrap();
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
                        ChoiceValue::Integer(v) => Some(i128::try_from(v.clone()).unwrap()),
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
    let initial: Vec<ChoiceNode> = (0..5)
        .map(|_| integer_node(1_000_000_000_000_000, 0, i128::MAX))
        .collect();
    let mut shrinker = group_accepts_uniform_at_least(initial, 100);
    shrinker.shrink_duplicates().unwrap();

    for n in &shrinker.current_nodes {
        match &n.value {
            ChoiceValue::Integer(v) => assert_eq!(i128::try_from(v.clone()).unwrap(), 100),
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
    let initial: Vec<ChoiceNode> = (0..5)
        .map(|_| integer_node(-1_000_000_000_000_000, i128::MIN + 1, 0))
        .collect();
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            ShrinkRun::Full(nodes) => {
                let int_vals: Vec<i128> = nodes
                    .iter()
                    .filter_map(|n| match &n.value {
                        ChoiceValue::Integer(v) => Some(i128::try_from(v.clone()).unwrap()),
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
    shrinker.shrink_duplicates().unwrap();

    for n in &shrinker.current_nodes {
        match &n.value {
            ChoiceValue::Integer(v) => assert_eq!(i128::try_from(v.clone()).unwrap(), -100),
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
/// test_fn always returns a length-1 truncation so after the first
/// group's replace, the OTHER group's `i < current_nodes.len()`
/// filter has both indices out of range and the branch fires.
/// Order-independent: whichever HashMap iteration hits first, the
/// second group's indices fall out of range.
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
            ShrinkRun::Full(_) => (true, vec![integer_node(0, 0, i128::MAX)], Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_duplicates().unwrap();
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
                let n = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
                    _ => 0,
                };
                if n <= 0 {
                    return (false, nodes.to_vec(), Spans::new());
                }
                (true, vec![nodes[0].clone()], Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_duplicates().unwrap();
}

/// Cover the outer-loop `valid.len() < 2 continue` inside
/// `shrink_duplicates`' integer-only second half.  The setup feeds two
/// integer groups; the first group's shift_right descent lands a
/// candidate that the test function accepts and truncates to a single
/// node, so by the time the outer loop advances to the second group
/// its indices fall out of range and the early-continue fires.
#[test]
fn shrink_duplicates_outer_skips_group_truncated_by_prior_group() {
    let initial = vec![
        integer_node(9, 0, i128::MAX),
        integer_node(9, 0, i128::MAX),
        integer_node(5, 0, i128::MAX),
        integer_node(5, 0, i128::MAX),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let head = match &nodes[0].value {
                    ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
                    _ => 0,
                };
                if nodes.len() == 4 {
                    if head > 7 {
                        return (true, nodes.to_vec(), Spans::new());
                    }
                    if head > 0 {
                        return (true, vec![nodes[0].clone()], Spans::new());
                    }
                    return (false, nodes.to_vec(), Spans::new());
                }
                (true, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_duplicates().unwrap();
    assert_eq!(shrinker.current_nodes.len(), 1);
}
