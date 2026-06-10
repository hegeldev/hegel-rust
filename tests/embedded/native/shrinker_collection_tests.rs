//! Unit tests for the `Collection.shrink` port, driving
//! `shrink_collection` directly with scripted access closures to reach
//! the defensive arms that real `Bytes`/`String` nodes only hit when the
//! test function truncates or puns the node mid-shrink.

use crate::native::core::choices::BytesChoice;
use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, Spans};
use crate::native::shrinker::{ShrinkRun, Shrinker};

use super::{CollectionAccess, probe};

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

fn bytes_access<'f>(
    read: &'f dyn Fn(&Shrinker<'_>) -> Option<Vec<u64>>,
    write: &'f dyn Fn(&[u64]) -> Option<ChoiceValue>,
) -> CollectionAccess<'f> {
    CollectionAccess { read, write }
}

fn standard_read(sh: &Shrinker<'_>) -> Option<Vec<u64>> {
    match sh.current_nodes.first().map(|n| &n.value) {
        Some(ChoiceValue::Bytes(v)) => Some(v.iter().map(|&b| u64::from(b)).collect()),
        _ => None,
    }
}

fn standard_write(keys: &[u64]) -> Option<ChoiceValue> {
    let mut out = Vec::with_capacity(keys.len());
    for &k in keys {
        out.push(u8::try_from(k).ok()?);
    }
    Some(ChoiceValue::Bytes(out))
}

/// `probe` rejects key vectors the access layer cannot represent.
#[test]
fn probe_rejects_unrepresentable_keys() {
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![bytes_node(vec![5])],
        Spans::new(),
    );
    let access = bytes_access(&standard_read, &standard_write);
    // 300 does not fit a byte, so `write` returns None and the probe is
    // rejected without running the test function.
    assert!(!probe(&mut shrinker, 0, &access, &[300]).unwrap());
}

/// A test function that truncates the realised value mid-pass exercises
/// the re-read guards: the deletion loop's empty-value break and the
/// per-element loop's bounds check.
#[test]
fn shrink_collection_handles_value_truncated_by_closure() {
    // Predicate: candidates of length >= 2 whose first byte is 9 are
    // "interesting", but the realised run always truncates to one byte —
    // so accepted candidates shrink the live value out from under the
    // loops that captured longer snapshots.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(v)) if v.first() == Some(&9)
                );
                let mut actual = nodes.to_vec();
                if let Some(node) = actual.first_mut() {
                    if let ChoiceValue::Bytes(v) = &mut node.value {
                        v.truncate(1);
                    }
                }
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![bytes_node(vec![9, 9, 9])],
        Spans::new(),
    );
    let access = bytes_access(&standard_read, &standard_write);
    shrinker.shrink_collection(0, 0, &access).unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => assert_eq!(v, &vec![9]),
        other => panic!("expected bytes, got {other:?}"),
    }
}

/// Gap sorting re-reads the live value between iterations; a closure that
/// shortens it mid-pass must not push the loop out of bounds.
#[test]
fn ordering_gap_loop_handles_shrinking_value() {
    // Start with an unsorted 4-byte value whose middle is pinned by the
    // predicate; the realised run drops the last byte once the value gets
    // smaller, shrinking the live length below the snapshot the gap loop
    // started from.
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run| match run {
            ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| &n.value),
                    Some(ChoiceValue::Bytes(v)) if v.len() >= 2 && v.contains(&7)
                );
                let mut actual = nodes.to_vec();
                if let Some(node) = actual.first_mut() {
                    if let ChoiceValue::Bytes(v) = &mut node.value {
                        if v.len() > 2 {
                            v.truncate(2);
                        }
                    }
                }
                (ok, actual, Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        vec![bytes_node(vec![3, 7, 2, 1])],
        Spans::new(),
    );
    let access = bytes_access(&standard_read, &standard_write);
    shrinker.shrink_collection(0, 0, &access).unwrap();
    match &shrinker.current_nodes[0].value {
        ChoiceValue::Bytes(v) => assert!(v.contains(&7), "kept the pinned byte: {v:?}"),
        other => panic!("expected bytes, got {other:?}"),
    }
}
