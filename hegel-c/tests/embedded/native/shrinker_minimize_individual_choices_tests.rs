//! Unit tests for `Shrinker::minimize_individual_choices`.

use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn forced_int_node(value: i128) -> ChoiceNode {
    let mut n = int_node(value);
    n.was_forced = true;
    n
}

fn int_value(node: &ChoiceNode) -> i128 {
    match &node.value {
        ChoiceValue::Integer(v) => i128::try_from(v.clone()).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn minimize_individual_choices_drives_int_to_simplest_when_predicate_admits() {
    let initial = vec![int_node(20)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0);
}

#[test]
fn minimize_individual_choices_skips_forced_nodes() {
    let initial = vec![forced_int_node(7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 7);
}

#[test]
fn minimize_individual_choices_invokes_span_delete_fallback() {
    let initial = vec![int_node(3), int_node(1), int_node(1), int_node(1)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let count = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let needed_len = 1 + count;
                let actual_len = needed_len.min(nodes.len());
                let actual_nodes = nodes[..actual_len].to_vec();
                let mut spans = Spans::new();
                if actual_len > 1 {
                    spans.push(Span {
                        start: 1,
                        end: actual_len,
                        label: "list".to_string(),
                        depth: 0,
                        parent: None,
                        discarded: false,
                    });
                }
                let ok = actual_len >= 2 && count >= 1;
                (ok, actual_nodes, spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn minimize_individual_choices_truncates_misaligned_string() {
    use crate::native::core::choices::StringChoice;
    use crate::native::intervalsets::IntervalSet;

    let initial = vec![
        int_node(3),
        ChoiceNode::new(
            ChoiceKind::String(StringChoice {
                intervals: IntervalSet::new(vec![(b'a' as u32, b'z' as u32)]).into(),
                min_size: 0,
                max_size: 16,
            }),
            ChoiceValue::String(vec![b'a' as u32, b'a' as u32, b'a' as u32]),
            false,
        ),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let n = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let candidate_str_len = match nodes.get(1).map(|n| &n.value) {
                    Some(ChoiceValue::String(s)) => s.len(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let mut actual: Vec<ChoiceNode> = nodes.to_vec();
                if let Some(node) = actual.get_mut(1) {
                    if let ChoiceValue::String(s) = &mut node.value {
                        s.truncate(n);
                    }
                }
                let ok = n >= 1 && candidate_str_len == n;
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert!(int_value(&shrinker.current_nodes[0]) < 3);
    match &shrinker.current_nodes[1].value {
        ChoiceValue::String(s) => {
            assert_eq!(
                s.len() as i128,
                int_value(&shrinker.current_nodes[0]),
                "string length should match the lowered integer"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn minimize_individual_choices_size_dep_single_node_delete_succeeds() {
    let initial = vec![int_node(2), int_node(7), int_node(7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let int_v = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let needed_len = 1usize.saturating_add(int_v as usize);
                let actual_len = needed_len.min(nodes.len());
                let actual: Vec<ChoiceNode> = nodes[..actual_len].to_vec();
                let ok = (int_v == 2 && actual.len() == 3) || (int_v == 1 && actual.len() == 1);
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn minimize_individual_choices_size_dep_span_delete_succeeds() {
    use crate::native::core::Span;
    let initial = vec![int_node(2), int_node(7), int_node(7), int_node(7)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let int_v = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let needed_len = 1usize.saturating_add(int_v as usize);
                let actual_len = needed_len.min(nodes.len());
                let actual: Vec<ChoiceNode> = nodes[..actual_len].to_vec();
                let mut spans = Spans::new();
                if actual.len() >= 2 {
                    spans.push(Span {
                        start: 1,
                        end: actual.len(),
                        label: "list".to_string(),
                        depth: 0,
                        parent: None,
                        discarded: false,
                    });
                }
                let ok = (int_v == 2 && actual.len() == 3) || (int_v == 1 && actual.len() == 1);
                (ok, actual, spans)
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 1);
    assert_eq!(shrinker.current_nodes.len(), 1);
}

#[test]
fn minimize_individual_choices_truncates_misaligned_bytes() {
    use crate::native::core::choices::BytesChoice;

    let initial = vec![
        int_node(3),
        ChoiceNode::new(
            ChoiceKind::Bytes(BytesChoice {
                min_size: 0,
                max_size: 16,
            }),
            ChoiceValue::Bytes(vec![1, 2, 3]),
            false,
        ),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let n = match nodes.first().map(|n| &n.value) {
                    Some(ChoiceValue::Integer(v)) => i128::try_from(v.clone()).unwrap() as usize,
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let candidate_len = match nodes.get(1).map(|n| &n.value) {
                    Some(ChoiceValue::Bytes(b)) => b.len(),
                    _ => return (false, nodes.to_vec(), Spans::new()),
                };
                let mut actual: Vec<ChoiceNode> = nodes.to_vec();
                if let Some(node) = actual.get_mut(1) {
                    if let ChoiceValue::Bytes(b) = &mut node.value {
                        b.truncate(n);
                    }
                }
                let ok = n >= 1 && candidate_len == n;
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert!(int_value(&shrinker.current_nodes[0]) < 3);
    match &shrinker.current_nodes[1].value {
        ChoiceValue::Bytes(b) => {
            assert_eq!(b.len() as i128, int_value(&shrinker.current_nodes[0]));
        }
        _ => unreachable!(),
    }
}

#[test]
fn minimize_individual_choices_no_op_on_already_simplest_node() {
    let initial = vec![int_node(0)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.minimize_individual_choices()).unwrap();
    assert_eq!(int_value(&shrinker.current_nodes[0]), 0);
}

#[test]
fn try_replace_with_deletion_continues_past_sizes_reaching_into_idx() {
    let initial = vec![int_node(1), int_node(2), int_node(3)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(_) => (false, vec![int_node(9)], Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    let deleted = drive_no_yield(shrinker.try_replace_with_deletion(
        2,
        ChoiceValue::Integer(BigInt::from(0)),
        5,
    ))
    .unwrap();
    assert!(!deleted);
}
