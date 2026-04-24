//! Ported from hypothesis-python/tests/quality/test_poisoned_trees.py.
//!
//! The Python original drives a `ConjectureRunner` end-to-end (generate +
//! `cached_test_function` splice + `shrink_interesting_examples`); this
//! port goes through the native `Shrinker` directly. We seed a specific
//! initial choice sequence encoding a "leftmost" tree of `size` leaves
//! with no poison values, shrink it, then for each leaf's `(n1, n2)` pair
//! splice in `(2**16 - 1, 2**16 - 1)` (which triggers the POISON branch)
//! plus a trailing marker and re-shrink. The assertion is the same as
//! the upstream: the shrunk poisoned tree is `(POISON,)`.
//!
//! The Python test parametrises over `seed` as well as `size`. Because
//! this port skips the random-generation phase and seeds a deterministic
//! initial tree, the seed axis collapses; only `size` remains.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    ChoiceKind, ChoiceNode, ChoiceValue, NativeTestCase, Shrinker,
};

const POISON_LEAF: Option<&str> = Some("POISON");
const MAX_INT: i128 = (1i128 << 32) - 1;
const MAX_16: i128 = (1i128 << 16) - 1;

/// Iterative pre-order tree draw. The pending counter walks the choice
/// sequence in the same order as the recursive Python `PoisonedTree.do_draw`.
fn draw_tree(ntc: &mut NativeTestCase, p: f64) -> Option<Vec<Option<&'static str>>> {
    let mut result = Vec::new();
    let mut pending = 1usize;
    while pending > 0 {
        pending -= 1;
        let split = ntc.weighted(p, None).ok()?;
        if split {
            pending += 2;
        } else {
            let n1 = ntc.draw_integer(0, MAX_16).ok()?;
            let n2 = ntc.draw_integer(0, MAX_16).ok()?;
            let n = (n1 << 16) | n2;
            result.push(if n == MAX_INT { POISON_LEAF } else { None });
        }
    }
    Some(result)
}

/// A leftmost-tree choice sequence with `size` leaves, all `(n1, n2) = (0, 0)`.
fn leftmost_tree_choices(size: usize) -> Vec<ChoiceValue> {
    let mut choices = Vec::with_capacity(4 * size - 1);
    for _ in 0..(size - 1) {
        choices.push(ChoiceValue::Boolean(true));
    }
    for _ in 0..size {
        choices.push(ChoiceValue::Boolean(false));
        choices.push(ChoiceValue::Integer(0));
        choices.push(ChoiceValue::Integer(0));
    }
    choices
}

fn values_of(nodes: &[ChoiceNode]) -> Vec<ChoiceValue> {
    nodes.iter().map(|n| n.value.clone()).collect()
}

fn run_poison(ntc: &mut NativeTestCase, p: f64, marker: &[u8]) -> bool {
    let tree = match draw_tree(ntc, p) {
        Some(t) => t,
        None => return false,
    };
    let m = match ntc.draw_bytes(marker.len(), marker.len()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    tree.contains(&POISON_LEAF) && m == marker
}

fn check_can_reduce_poison_from_any_subtree(size: usize) {
    let p = 1.0 / (2.0 - 1.0 / (size as f64));
    let marker: Vec<u8> = vec![1, 2, 3, 4];

    // Phase 1: seed a size-`size` no-poison tree and shrink to the minimum.
    let initial = leftmost_tree_choices(size);
    let mut ntc = NativeTestCase::for_choices(&initial, None);
    draw_tree(&mut ntc, p).unwrap();
    let initial_nodes = ntc.nodes.clone();

    let phase1_fn = Box::new(move |candidate: &[ChoiceNode]| {
        let values = values_of(candidate);
        let mut ntc = NativeTestCase::for_choices(&values, Some(candidate));
        let interesting = match draw_tree(&mut ntc, p) {
            Some(t) => t.len() >= size,
            None => false,
        };
        (interesting, ntc.nodes.clone())
    });
    let mut shrinker = Shrinker::new(phase1_fn, initial_nodes);
    shrinker.shrink();
    let shrunk_nodes = shrinker.current_nodes.clone();
    let shrunk_values = values_of(&shrunk_nodes);

    // Upstream asserts the shrinker converges to a tree of exactly `size`
    // leaves (the minimum satisfying `len >= size`).
    let mut replay = NativeTestCase::for_choices(&shrunk_values, None);
    let shrunk_tree = draw_tree(&mut replay, p).unwrap();
    assert_eq!(shrunk_tree.len(), size);

    // Find each integer leaf node (max_value == 2^16 - 1) in order.
    let leaf_integer_indices: Vec<usize> = shrunk_nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| match &n.kind {
            ChoiceKind::Integer(k) if k.max_value == MAX_16 => Some(i),
            _ => None,
        })
        .collect();
    assert_eq!(leaf_integer_indices.len() % 2, 0);

    // Phase 2: for each leaf, splice poison into its (n1, n2) pair plus a
    // trailing marker and re-shrink. The shrunk result must be (POISON,).
    for pair in leaf_integer_indices.chunks_exact(2) {
        let node_index = pair[0];

        let mut poisoned: Vec<ChoiceValue> = shrunk_values[..node_index].to_vec();
        poisoned.push(ChoiceValue::Integer(MAX_16));
        poisoned.push(ChoiceValue::Integer(MAX_16));
        poisoned.extend_from_slice(&shrunk_values[node_index + 2..]);
        poisoned.push(ChoiceValue::Bytes(marker.clone()));

        let mut ntc = NativeTestCase::for_choices(&poisoned, None);
        assert!(
            run_poison(&mut ntc, p, &marker),
            "poison splice at {node_index} should be interesting"
        );
        let poison_initial_nodes = ntc.nodes.clone();

        let marker_for_shrinker = marker.clone();
        let phase2_fn = Box::new(move |candidate: &[ChoiceNode]| {
            let values = values_of(candidate);
            let mut ntc = NativeTestCase::for_choices(&values, Some(candidate));
            let interesting = run_poison(&mut ntc, p, &marker_for_shrinker);
            (interesting, ntc.nodes.clone())
        });
        let mut poison_shrinker = Shrinker::new(phase2_fn, poison_initial_nodes);
        poison_shrinker.shrink();

        let shrunk_poison_values = values_of(&poison_shrinker.current_nodes);
        let mut replay = NativeTestCase::for_choices(&shrunk_poison_values, None);
        let final_tree = draw_tree(&mut replay, p).unwrap();
        assert_eq!(
            final_tree,
            vec![POISON_LEAF],
            "poison at leaf index {node_index} should shrink to (POISON,)"
        );
    }
}

#[test]
fn test_can_reduce_poison_from_any_subtree_size_2() {
    check_can_reduce_poison_from_any_subtree(2);
}

#[test]
fn test_can_reduce_poison_from_any_subtree_size_5() {
    check_can_reduce_poison_from_any_subtree(5);
}

#[test]
fn test_can_reduce_poison_from_any_subtree_size_10() {
    check_can_reduce_poison_from_any_subtree(10);
}
