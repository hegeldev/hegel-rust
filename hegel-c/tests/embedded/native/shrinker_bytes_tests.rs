use super::*;
use crate::exchange::drive_no_yield;
use crate::native::core::{ChoiceNode, Spans};
use crate::native::shrinker::Shrinker;

fn bytes_node(value: Vec<u8>, min_size: usize, max_size: usize) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Bytes(BytesChoice { min_size, max_size }),
        ChoiceValue::Bytes(value),
        false,
    )
}

fn accepting_shrinker(nodes: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run: crate::native::shrinker::ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        nodes,
        Spans::new(),
    )
}

#[test]
fn shrink_bytes_collapses_accepting_run_to_simplest() {
    let initial = vec![bytes_node(vec![3, 1, 4, 1], 1, 10)];
    let mut shrinker = accepting_shrinker(initial);
    drive_no_yield(shrinker.shrink_bytes()).unwrap();
    let v = match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => v.clone(),
        _ => unreachable!(),
    };
    assert_eq!(v, vec![0u8]);
}

#[test]
fn shrink_bytes_linear_scan_breaks_when_replace_shortens_below_sz() {
    let initial = vec![bytes_node(vec![7, 0, 0, 0, 0, 0, 0, 0], 0, 16)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: crate::native::shrinker::ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let is_singleton_seven = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(b)) if b.as_slice() == [7]
                );
                (is_singleton_seven, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_bytes()).unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => assert_eq!(v, &vec![7u8]),
        _ => unreachable!(),
    }
}

#[test]
fn redistribute_bytes_pair_partial_move_triggers_bin_search() {
    let initial = vec![
        bytes_node(vec![1, 2, 3], 0, 10),
        bytes_node(vec![4, 5], 0, 10),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: crate::native::shrinker::ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let t_ok = matches!(
                    nodes.get(1).map(|n| &n.value),
                    Some(ChoiceValue::Bytes(b)) if b.len() <= 3
                );
                (t_ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.redistribute_bytes_pairs()).unwrap();
    match &shrinker.current_nodes[1].value {
        ChoiceValue::Bytes(b) => assert!(b.len() <= 3, "t exceeded 3 bytes: {b:?}"),
        _ => unreachable!(),
    }
}

#[test]
fn redistribute_bytes_pair_moves_several_elements_in_one_invocation() {
    let initial = vec![
        bytes_node(vec![1, 2, 3, 4, 5, 6, 7, 8], 0, 10),
        bytes_node(vec![9], 0, 10),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: crate::native::shrinker::ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let s_ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(b)) if !b.is_empty()
                );
                (s_ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.redistribute_bytes_pairs()).unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(b) => assert_eq!(
            b,
            &vec![1],
            "one invocation should move every element the predicate allows"
        ),
        _ => unreachable!(),
    }
}

#[test]
fn redistribute_bytes_pair_moves_entire_value_when_accepted() {
    let initial = vec![
        bytes_node(vec![1, 2, 3], 0, 10),
        bytes_node(vec![4, 5], 0, 10),
    ];
    let mut shrinker = accepting_shrinker(initial);
    drive_no_yield(shrinker.redistribute_bytes_pairs()).unwrap();
    let (a, b) = match (
        &shrinker.current_nodes[0].value,
        &shrinker.current_nodes[1].value,
    ) {
        (ChoiceValue::Bytes(a), ChoiceValue::Bytes(b)) => (a.clone(), b.clone()),
        _ => unreachable!(),
    };
    assert!(a.is_empty(), "first node not emptied: {a:?}");
    assert_eq!(b, vec![1, 2, 3, 4, 5]);
}
