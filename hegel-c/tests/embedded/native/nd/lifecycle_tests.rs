//! Embedded tests for `src/native/nd/lifecycle.rs`.

use super::*;
use crate::native::HashMap;
use crate::native::core::Status;
use alloc::vec;
use alloc::vec::Vec;

fn witness(origin: &str) -> RunResult {
    RunResult {
        status: Status::Interesting,
        nodes: Vec::new(),
        spans: Vec::new(),
        origin: Some(origin.to_string()),
        target_observations: HashMap::default(),
        span_events: Vec::new(),
        events: Vec::new(),
    }
}

#[test]
fn observed_origin_needs_confirmation_and_is_evictable() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.needs_confirmation("a"));
    lc.observe("a");
    assert!(lc.needs_confirmation("a"));
    assert!(lc.take_witness("a").is_none());
    assert!(lc.pool("a").is_empty());
    assert!(lc.reject("a"));
}

#[test]
fn observe_never_resets_rejection_counts() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    assert!(lc.reject("a"));
    lc.observe("a");
    assert!(lc.reject("a"));
    assert_eq!(lc.unconfirmed().collect::<Vec<_>>(), vec![("a", 2)]);
}

#[test]
fn rejecting_an_unobserved_origin_records_it() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.reject("a"));
    assert_eq!(lc.unconfirmed().collect::<Vec<_>>(), vec![("a", 1)]);
}

#[test]
fn confirmation_stores_replay_state_and_the_witness_is_taken_once() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    lc.confirm("a", 0.4, Some(witness("a")), vec![Vec::new()]);
    assert!(!lc.needs_confirmation("a"));
    assert_eq!(lc.pool("a").len(), 1);
    let (run, anchor) = lc.take_witness("a").unwrap();
    assert_eq!(run.origin.as_deref(), Some("a"));
    assert_eq!(anchor, 0.4);
    assert!(lc.take_witness("a").is_none());
    assert!(!lc.reject("a"));
    assert_eq!(lc.unconfirmed().count(), 0);
}

#[test]
fn trusted_origins_survive_rejection_without_a_caveat() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    lc.trust("a", Vec::new());
    assert!(!lc.needs_confirmation("a"));
    assert!(!lc.reject("a"));
    assert_eq!(lc.unconfirmed().count(), 0);
    assert!(lc.take_witness("a").is_none());
}

#[test]
fn trust_carries_a_stored_pool_and_never_replaces_it_with_an_empty_one() {
    let mut lc = OriginLifecycle::default();
    lc.trust("a", vec![vec![ChoiceValue::Boolean(true)]]);
    assert_eq!(lc.pool("a").len(), 1);
    lc.trust("a", Vec::new());
    assert_eq!(lc.pool("a").len(), 1);
    lc.trust("a", vec![Vec::new(), vec![ChoiceValue::Boolean(false)]]);
    assert_eq!(lc.pool("a").len(), 2);
}

#[test]
fn trust_is_idempotent_and_covers_unobserved_origins() {
    let mut lc = OriginLifecycle::default();
    lc.trust("a", Vec::new());
    lc.trust("a", Vec::new());
    assert!(!lc.needs_confirmation("a"));
}

#[test]
fn trust_never_demotes_a_confirmed_origin() {
    let mut lc = OriginLifecycle::default();
    lc.confirm("a", 0.7, Some(witness("a")), Vec::new());
    lc.trust("a", vec![Vec::new()]);
    let (_, anchor) = lc.take_witness("a").unwrap();
    assert_eq!(anchor, 0.7);
    assert!(lc.pool("a").is_empty());
}

#[test]
fn unconfirmed_report_is_sorted_and_skips_confirmed_origins() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.reject("c"));
    assert!(lc.reject("a"));
    assert!(lc.reject("a"));
    lc.observe("b");
    lc.confirm("d", 0.5, None, Vec::new());
    assert_eq!(
        lc.unconfirmed().collect::<Vec<_>>(),
        vec![("a", 2), ("c", 1)]
    );
}
