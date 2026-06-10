use super::*;
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
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        nodes,
        Spans::new(),
    )
}

#[test]
fn shrink_bytes_collapses_accepting_run_to_simplest() {
    // An always-accepting test_fn drives the shrinker from a 4-byte value to
    // the simplest (single zero) — exercising the simplest replace and the
    // surrounding loop structure.
    let initial = vec![bytes_node(vec![3, 1, 4, 1], 1, 10)];
    let mut shrinker = accepting_shrinker(initial);
    shrinker.shrink_bytes().unwrap();
    let v = match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => v.clone(),
        _ => unreachable!(),
    };
    assert_eq!(v, vec![0u8]);
}

#[test]
fn redistribute_bytes_pair_partial_move_triggers_bin_search() {
    // Reach the binary-search arm of `redistribute_bytes_pairs` that scans
    // for the longest suffix of `s` movable into `t`. The full-move step
    // must fail (so the early `return` after `combined` does not fire) and
    // the single-byte step must succeed (otherwise the second `return`
    // exits before bin_search).
    //
    // Predicate: accept iff `nodes[1]` has at most 3 bytes. Full-move
    // builds `t = [1,2,3,4,5]` (rejected); single-byte move builds
    // `t = [3,4,5]` (accepted). bin_search then probes for the longest
    // movable suffix, which executes the loop body and `replace` call.
    let initial = vec![
        bytes_node(vec![1, 2, 3], 0, 10),
        bytes_node(vec![4, 5], 0, 10),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
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
    shrinker.redistribute_bytes_pairs().unwrap();
    match &shrinker.current_nodes[1].value {
        ChoiceValue::Bytes(b) => assert!(b.len() <= 3, "t exceeded 3 bytes: {b:?}"),
        _ => unreachable!(),
    }
}

#[test]
fn redistribute_bytes_pair_moves_entire_value_when_accepted() {
    // Adjacent `BytesChoice` pair: the accepting test_fn lets the first
    // step (move everything from `s` to `t`) succeed, exercising the early
    // `return` after that branch's success path.
    let initial = vec![
        bytes_node(vec![1, 2, 3], 0, 10),
        bytes_node(vec![4, 5], 0, 10),
    ];
    let mut shrinker = accepting_shrinker(initial);
    shrinker.redistribute_bytes_pairs().unwrap();
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

// ----- Collection.shrink ports (Hypothesis shrinking/collection.py) -----

fn bytes_value(sh: &Shrinker<'_>, i: usize) -> Vec<u8> {
    match &sh.current_nodes[i].value {
        ChoiceValue::Bytes(v) => v.clone(),
        _ => unreachable!(),
    }
}

/// The all-simplest-at-current-length probe plus joint duplicate
/// minimization: with "all three bytes equal" as the predicate, changing
/// any single byte breaks it, so only a simultaneous move reaches zero.
#[test]
fn shrink_bytes_collapses_linked_equal_bytes_together() {
    let initial = vec![bytes_node(vec![7, 7, 7], 0, 10)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(v))
                        if v.len() == 3 && v.iter().all(|&b| b == v[0])
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_bytes().unwrap();
    assert_eq!(bytes_value(&shrinker, 0), vec![0, 0, 0]);
}

/// Per-element minimization must use the full Integer move set: the low
/// nibble of the byte is constrained (`b & 0x0F == 0x0E`), which the old
/// monotone binary search converges on only by luck of its midpoints
/// (settling at 142 from 254), while `mask_high_bits` walks straight down
/// to the minimal value 14.
#[test]
fn shrink_bytes_per_element_uses_integer_moves() {
    let initial = vec![bytes_node(vec![254], 1, 4)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(v)) if v.len() == 1 && v[0] & 0x0F == 0x0E
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_bytes().unwrap();
    assert_eq!(bytes_value(&shrinker, 0), vec![14]);
}

/// Deletion is adaptive (find_integer-chunked, back to front): a run of 64
/// deletable bytes ahead of the byte that matters costs O(log n) calls,
/// not O(n) for the failed prefix-truncations plus O(n) single deletions.
#[test]
fn shrink_bytes_deletion_is_adaptive() {
    use std::cell::Cell;
    use std::rc::Rc;
    let calls = Rc::new(Cell::new(0usize));
    let calls_clone = calls.clone();
    let mut value: Vec<u8> = vec![9; 64];
    value.push(255);
    let initial = vec![bytes_node(value, 0, 100)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                calls_clone.set(calls_clone.get() + 1);
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(v)) if v.contains(&255)
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    shrinker.shrink_bytes().unwrap();
    assert_eq!(bytes_value(&shrinker, 0), vec![255]);
    assert!(
        calls.get() < 60,
        "deletion should be adaptive; took {} calls",
        calls.get()
    );
}
