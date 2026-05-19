use super::*;
use crate::native::core::ChoiceNode;
use crate::native::shrinker::Shrinker;

fn bytes_node(value: Vec<u8>, min_size: usize, max_size: usize) -> ChoiceNode {
    ChoiceNode {
        kind: ChoiceKind::Bytes(BytesChoice { min_size, max_size }),
        value: ChoiceValue::Bytes(value),
        was_forced: false,
    }
}

fn accepting_shrinker(nodes: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        nodes,
    )
}

#[test]
fn shrink_bytes_collapses_accepting_run_to_simplest() {
    // An always-accepting test_fn drives the shrinker from a 4-byte value to
    // the simplest (single zero) — exercising the simplest replace and the
    // surrounding loop structure.
    let initial = vec![bytes_node(vec![3, 1, 4, 1], 1, 10)];
    let mut shrinker = accepting_shrinker(initial);
    shrinker.shrink_bytes();
    let v = match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => v.clone(),
        _ => unreachable!(),
    };
    assert_eq!(v, vec![0u8]);
}

#[test]
fn shrink_bytes_linear_scan_breaks_when_replace_shortens_below_sz() {
    // The linear-scan fallback's `sz > cur.len()` guard only fires when a
    // mid-loop `replace` shortens the current value below the next index.
    //
    // Setup: an 8-byte value whose only accepted shape is the singleton
    // `[7]`. `simplest` (an empty vec) is rejected; `bin_search_down`
    // probes mid points of `min_size..cur_len = 0..8` — `f(0)`, `f(4)`,
    // `f(6)`, `f(7)` — and never tries `sz == 1`, so the linear scan still
    // sees the full 8-byte value. Scan iteration `sz == 1` then accepts
    // and replaces `cur` with `[7]`, and `sz == 2` immediately hits the
    // break (`2 > cur.len() == 1`).
    let initial = vec![bytes_node(vec![7, 0, 0, 0, 0, 0, 0, 0], 0, 16)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let is_singleton_seven = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(b)) if b.as_slice() == [7]
                );
                (is_singleton_seven, nodes.to_vec())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        initial,
    );
    shrinker.shrink_bytes();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => assert_eq!(v, &vec![7u8]),
        _ => unreachable!(),
    }
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
                (t_ok, nodes.to_vec())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new()),
        }),
        initial,
    );
    shrinker.redistribute_bytes_pairs();
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
    shrinker.redistribute_bytes_pairs();
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
