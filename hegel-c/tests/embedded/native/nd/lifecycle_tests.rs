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
    assert!(lc.reject("a", (0, 10)));
}

#[test]
fn observe_never_resets_rejection_counts() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    assert!(lc.reject("a", (1, 10)));
    lc.observe("a");
    assert!(lc.reject("a", (2, 30)));
    assert_eq!(lc.unconfirmed().collect::<Vec<_>>(), vec![("a", 2)]);
}

#[test]
fn rejecting_an_unobserved_origin_records_it() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.reject("a", (0, 10)));
    assert_eq!(lc.unconfirmed().collect::<Vec<_>>(), vec![("a", 1)]);
}

#[test]
fn confirmation_stores_replay_state_and_the_witness_is_taken_once() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    lc.confirm("a", 0.4, Some(witness("a")), vec![Vec::new()], (4, 9));
    assert!(!lc.needs_confirmation("a"));
    assert_eq!(lc.pool("a").len(), 1);
    let (run, anchor) = lc.take_witness("a").unwrap();
    assert_eq!(run.origin.as_deref(), Some("a"));
    assert_eq!(anchor, 0.4);
    assert!(lc.take_witness("a").is_none());
    assert!(!lc.reject("a", (0, 10)));
    assert_eq!(lc.unconfirmed().count(), 0);
}

#[test]
fn trusted_origins_survive_rejection_without_a_caveat() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    lc.trust("a", Vec::new());
    assert!(!lc.needs_confirmation("a"));
    assert!(!lc.reject("a", (0, 10)));
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
    lc.confirm("a", 0.7, Some(witness("a")), Vec::new(), (4, 4));
    lc.trust("a", vec![Vec::new()]);
    let (_, anchor) = lc.take_witness("a").unwrap();
    assert_eq!(anchor, 0.7);
    assert!(lc.pool("a").is_empty());
}

#[test]
fn unconfirmed_report_is_sorted_and_skips_confirmed_origins() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.reject("c", (0, 10)));
    assert!(lc.reject("a", (0, 10)));
    assert!(lc.reject("a", (0, 10)));
    lc.observe("b");
    lc.confirm("d", 0.5, None, Vec::new(), (4, 6));
    assert_eq!(
        lc.unconfirmed().collect::<Vec<_>>(),
        vec![("a", 2), ("c", 1)]
    );
}

#[test]
fn raise_anchor_is_monotone_and_confirmed_only() {
    let mut lc = OriginLifecycle::default();
    lc.raise_anchor("a", 0.9);
    lc.observe("a");
    lc.raise_anchor("a", 0.9);
    assert!(lc.needs_confirmation("a"));
    lc.trust("b", Vec::new());
    lc.raise_anchor("b", 0.9);
    assert!(lc.take_witness("b").is_none());
    lc.confirm("c", 0.3, Some(witness("c")), Vec::new(), (4, 12));
    lc.raise_anchor("c", 0.2);
    lc.raise_anchor("c", 0.6);
    let (_, anchor) = lc.take_witness("c").unwrap();
    assert_eq!(anchor, 0.6);
}

#[test]
fn caveats_quote_the_accumulated_replay_evidence() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.caveat("a").is_none());
    lc.observe("a");
    assert!(lc.reject("a", (1, 40)));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "unconfirmed failure: failed 1 of 40 replays this run, below the \
         confirmation bar — likely rare"
    );
    assert!(lc.reject("a", (0, 10)));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "unconfirmed failure: failed 1 of 50 replays this run, below the \
         confirmation bar — likely rare"
    );
    lc.confirm("a", 0.3, None, Vec::new(), (4, 12));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed: failed 5 of 62 replays this run"
    );
    lc.trust("b", Vec::new());
    assert_eq!(
        lc.caveat("b").unwrap(),
        "nondeterministic failure: reproduced from the stored entry this run"
    );
}

#[test]
fn a_dry_final_replay_switches_the_confirmed_caveat_wording() {
    let mut lc = OriginLifecycle::default();
    lc.record_final_replay("a", (0, 29));
    assert!(lc.caveat("a").is_none());
    lc.confirm("a", 0.4, None, Vec::new(), (4, 9));
    lc.record_final_replay("a", (1, 3));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed: failed 5 of 12 replays this run"
    );
    lc.record_final_replay("a", (0, 29));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed earlier this run (failed 5 of 41 \
         replays) but not reproduced at report time — a rare failure, or \
         something in the environment changed after discovery"
    );
}
