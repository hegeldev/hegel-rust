//! Unit tests for `Shrinker::pass_to_descendant`.

use crate::native::bignum::BigInt;
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Span, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Integer(IntegerChoice {
            min_value: BigInt::from(i128::MIN),
            max_value: BigInt::from(i128::MAX),
            shrink_towards: BigInt::from(0),
        }),
        ChoiceValue::Integer(BigInt::from(value)),
        false,
    )
}

fn lab(start: usize, end: usize, label: &str) -> Span {
    Span {
        start,
        end,
        label: label.to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    }
}

#[test]
fn pass_to_descendant_replaces_outer_with_inner_same_label() {
    // current_nodes = [a, b, c, d, e]
    // spans:
    //   * 0..5 label="tree"  ← ancestor
    //   * 2..4 label="tree"  ← descendant (length 2 < 5)
    // After replacement: [a, b, c, d] would not be reachable.  Expected
    // outcome: prefix [0..0] + descendant [2..4] + suffix [5..] = [c, d].
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
    ];
    let mut spans = Spans::new();
    spans.push(lab(0, 5, "tree"));
    spans.push(lab(2, 4, "tree"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.pass_to_descendant().unwrap();

    let values: Vec<_> = shrinker
        .current_nodes
        .iter()
        .map(|n| match &n.value {
            ChoiceValue::Integer(v) => i128::try_from(v).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(values, vec![3, 4]);
}

#[test]
fn pass_to_descendant_skips_different_labels() {
    // Even though the inner span is shorter, its label differs from the
    // ancestor's, so pass_to_descendant must leave them alone.
    let initial = vec![int_node(1), int_node(2), int_node(3)];
    let mut spans = Spans::new();
    spans.push(lab(0, 3, "outer"));
    spans.push(lab(1, 2, "inner"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.pass_to_descendant().unwrap();
    // Nothing changed.
    assert_eq!(shrinker.current_nodes.len(), 3);
}

#[test]
fn pass_to_descendant_skips_equal_length_descendant() {
    // A "descendant" whose length matches the ancestor isn't smaller,
    // so the replacement would not be a shrink.  Even if predicates would
    // accept it, the pass declines.
    let initial = vec![int_node(7), int_node(8)];
    let mut spans = Spans::new();
    spans.push(lab(0, 2, "tree"));
    spans.push(lab(0, 2, "tree")); // same range — choice_count not strictly less.

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.pass_to_descendant().unwrap();
    assert_eq!(shrinker.current_nodes.len(), 2);
}

#[test]
fn pass_to_descendant_handles_multiple_descendants() {
    // Three spans of the same label, all nested.  The pass should try the
    // smallest first (or any of them).  The closure rejects any
    // replacement that would leave behind more than 2 elements; only the
    // innermost (length 1) span works.
    let initial = vec![
        int_node(1),
        int_node(2),
        int_node(3),
        int_node(4),
        int_node(5),
        int_node(6),
    ];
    let mut spans = Spans::new();
    spans.push(lab(0, 6, "tree")); // outer
    spans.push(lab(1, 4, "tree")); // middle (length 3)
    spans.push(lab(2, 3, "tree")); // inner (length 1)

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (nodes.len() <= 2, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.pass_to_descendant().unwrap();
    // The innermost span (length 1) gives prefix [] + [3] + suffix [] when
    // replacing the outermost; that's accepted (len 1 ≤ 2).
    assert!(shrinker.current_nodes.len() <= 2);
}

#[test]
fn pass_to_descendant_safe_when_indices_outrange_after_shrink() {
    // current_nodes is shorter than the span boundaries suggest (e.g. the
    // closure returned a shorter actual run).  pass_to_descendant must
    // not panic.
    let initial = vec![int_node(1), int_node(2)];
    let mut spans = Spans::new();
    spans.push(lab(0, 5, "tree")); // a_end > nodes.len()
    spans.push(lab(1, 3, "tree"));

    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        spans,
    );
    shrinker.pass_to_descendant().unwrap();
    assert_eq!(shrinker.current_nodes.len(), 2);
}
