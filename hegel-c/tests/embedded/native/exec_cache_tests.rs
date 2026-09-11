use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice};

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::boolean(BooleanChoice { p: 0.5 }, value, false)
}

fn int_node(min: i128, value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(min),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

#[test]
fn a_recorded_conclusion_is_served_on_exact_repeat() {
    let mut cache = ExecCache::default();
    let nodes = alloc::vec![bool_node(true)];
    let recorded = cache.record(
        alloc::vec![1, 2, 3],
        Status::Interesting,
        Some("origin"),
        &nodes,
        &[],
        true,
    );
    assert!(!recorded.duplicate);
    assert!(!recorded.verdict_mismatch);
    let hit = cache.serve(&[1, 2, 3]).unwrap();
    assert_eq!(hit.status, Status::Interesting);
    assert_eq!(hit.origin.as_deref(), Some("origin"));
    assert_eq!(hit.nodes.len(), 1);
    assert!(cache.serve(&[9, 9]).is_none());
}

#[test]
fn the_generation_tier_detects_duplicates_without_keeping_serving_entries() {
    let mut cache = ExecCache::default();
    let first = cache.record(alloc::vec![7], Status::Valid, None, &[], &[], false);
    assert!(!first.duplicate);
    assert!(cache.serve(&[7]).is_none());
    let repeat = cache.record(alloc::vec![7], Status::Valid, None, &[], &[], false);
    assert!(repeat.duplicate);
    assert!(!repeat.verdict_mismatch);
}

#[test]
fn a_verdict_change_on_a_repeat_is_a_mismatch() {
    let mut cache = ExecCache::default();
    cache.record(alloc::vec![7], Status::Valid, None, &[], &[], false);
    let flipped = cache.record(
        alloc::vec![7],
        Status::Interesting,
        Some("o"),
        &[],
        &[],
        true,
    );
    assert!(flipped.duplicate);
    assert!(flipped.verdict_mismatch);
    assert!(
        cache.serve(&[7]).is_none(),
        "a contradicted conclusion must not become servable"
    );
}

#[test]
fn an_origin_change_alone_is_a_mismatch() {
    let mut cache = ExecCache::default();
    cache.record(
        alloc::vec![7],
        Status::Interesting,
        Some("a"),
        &[],
        &[],
        false,
    );
    let moved = cache.record(
        alloc::vec![7],
        Status::Interesting,
        Some("b"),
        &[],
        &[],
        false,
    );
    assert!(moved.verdict_mismatch);
}

#[test]
fn the_execution_cache_is_bounded() {
    let mut cache = ExecCache::with_full_tier_bound(1);
    let key: Vec<u8> = (0..100).collect();
    cache.record(key.clone(), Status::Valid, None, &[], &[], true);
    assert!(
        cache.serve(&key).is_none(),
        "an entry over the whole budget is evicted immediately"
    );
    assert_eq!(cache.full_bytes, 0);
}

#[test]
fn eviction_drops_the_oldest_entry_first() {
    let entry_cost = 100 + core::mem::size_of::<CachedRun>();
    let mut cache = ExecCache::with_full_tier_bound(2 * entry_cost);
    let key = |tag: u8| -> Vec<u8> { core::iter::repeat_n(tag, 100).collect() };
    cache.record(key(1), Status::Valid, None, &[], &[], true);
    cache.record(key(2), Status::Valid, None, &[], &[], true);
    cache.record(key(3), Status::Valid, None, &[], &[], true);
    assert!(cache.serve(&key(1)).is_none());
    assert!(cache.serve(&key(2)).is_some());
    assert!(cache.serve(&key(3)).is_some());
}

#[test]
fn clear_drops_both_tiers() {
    let mut cache = ExecCache::default();
    cache.record(alloc::vec![7], Status::Valid, None, &[], &[], true);
    cache.clear();
    assert!(cache.serve(&[7]).is_none());
    let again = cache.record(
        alloc::vec![7],
        Status::Interesting,
        Some("o"),
        &[],
        &[],
        true,
    );
    assert!(!again.duplicate);
    assert!(!again.verdict_mismatch);
}

#[test]
fn the_kind_ledger_accepts_consistent_reexecution() {
    let mut ledger = KindLedger::default();
    let nodes = alloc::vec![int_node(0, 5), bool_node(true)];
    assert!(ledger.observe(&nodes).unwrap().is_none());
    assert!(ledger.observe(&nodes).unwrap().is_none());
    let other_values = alloc::vec![int_node(0, 6), bool_node(false)];
    assert!(ledger.observe(&other_values).unwrap().is_none());
}

#[test]
fn the_kind_ledger_reports_kind_drift_at_a_shared_prefix() {
    let mut ledger = KindLedger::default();
    assert!(ledger.observe(&[bool_node(true)]).unwrap().is_none());
    let msg = ledger.observe(&[int_node(0, 5)]).unwrap().unwrap();
    assert!(msg.contains("choice kind changed from"), "{msg}");
    assert!(msg.contains("global mutable state"), "{msg}");
}

#[test]
fn the_kind_ledger_treats_a_constraint_change_as_kind_drift() {
    let mut ledger = KindLedger::default();
    assert!(ledger.observe(&[int_node(0, 5)]).unwrap().is_none());
    assert!(ledger.observe(&[int_node(1, 5)]).unwrap().is_some());
}

#[test]
fn kind_drift_is_scoped_to_the_value_prefix() {
    let mut ledger = KindLedger::default();
    assert!(
        ledger
            .observe(&[bool_node(true), int_node(0, 5)])
            .unwrap()
            .is_none()
    );
    assert!(
        ledger
            .observe(&[bool_node(false), bool_node(true)])
            .unwrap()
            .is_none(),
        "a different position-0 value opens an independent prefix"
    );
    assert!(
        ledger
            .observe(&[bool_node(true), bool_node(true)])
            .unwrap()
            .is_some(),
        "the same position-0 value must reuse the recorded prefix"
    );
}

#[test]
fn the_kind_ledger_stops_learning_at_its_cap_but_keeps_checking() {
    let mut ledger = KindLedger::default();
    let long: Vec<ChoiceNode> = (0..KIND_LEDGER_CAP + 10).map(|_| bool_node(true)).collect();
    assert!(ledger.observe(&long).unwrap().is_none());
    assert_eq!(ledger.entries.len(), KIND_LEDGER_CAP);
    assert!(ledger.observe(&[int_node(0, 5)]).unwrap().is_some());
    ledger.clear();
    assert!(ledger.observe(&[int_node(0, 5)]).unwrap().is_none());
}
