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
    assert_eq!(lc.unconfirmed().collect::<Vec<_>>(), vec!["a"]);
    assert_eq!(
        lc.caveat("a").unwrap(),
        "unconfirmed failure: failed 3 of 40 replays this run, below the \
         confirmation bar — likely rare"
    );
}

#[test]
fn rejecting_an_unobserved_origin_records_it() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.reject("a", (0, 10)));
    assert_eq!(lc.unconfirmed().collect::<Vec<_>>(), vec!["a"]);
}

#[test]
fn confirmation_stores_replay_state_and_the_witness_is_taken_once() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    lc.confirm("a", 0.4, Some(witness("a")), vec![Vec::new()], (4, 9))
        .unwrap();
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
fn trusted_origins_survive_rejection_without_eviction() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    lc.trust("a", Vec::new(), (1, 2));
    assert!(!lc.needs_confirmation("a"));
    assert!(!lc.reject("a", (0, 10)));
    assert_eq!(lc.unconfirmed().count(), 0);
    assert!(lc.take_witness("a").is_none());
}

#[test]
fn trust_carries_a_stored_pool_and_never_replaces_it_with_an_empty_one() {
    let mut lc = OriginLifecycle::default();
    lc.trust("a", vec![vec![ChoiceValue::Boolean(true)]], (1, 1));
    assert_eq!(lc.pool("a").len(), 1);
    lc.trust("a", Vec::new(), (1, 1));
    assert_eq!(lc.pool("a").len(), 1);
    lc.trust(
        "a",
        vec![Vec::new(), vec![ChoiceValue::Boolean(false)]],
        (1, 1),
    );
    assert_eq!(lc.pool("a").len(), 2);
}

#[test]
fn trust_seeds_and_folds_reuse_evidence() {
    let mut lc = OriginLifecycle::default();
    lc.trust("a", Vec::new(), (1, 4));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, reproduced from stored timelines: failed \
         1 of 4 replays this run"
    );
    lc.trust("a", Vec::new(), (2, 3));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, reproduced from stored timelines: failed \
         3 of 7 replays this run"
    );
    assert!(!lc.needs_confirmation("a"));
}

#[test]
fn trust_never_demotes_a_confirmed_origin() {
    let mut lc = OriginLifecycle::default();
    lc.confirm("a", 0.7, Some(witness("a")), Vec::new(), (4, 4))
        .unwrap();
    lc.trust("a", vec![Vec::new()], (1, 1));
    let (_, anchor) = lc.take_witness("a").unwrap();
    assert_eq!(anchor, 0.7);
    assert!(lc.pool("a").is_empty());
}

#[test]
fn confirm_on_a_confirmed_origin_is_an_internal_error() {
    let mut lc = OriginLifecycle::default();
    lc.confirm("a", 0.4, None, Vec::new(), (4, 9)).unwrap();
    assert!(lc.confirm("a", 0.5, None, Vec::new(), (4, 4)).is_err());
}

#[test]
fn promotion_folds_trusted_evidence_into_the_confirmed_counts() {
    let mut lc = OriginLifecycle::default();
    lc.trust("a", Vec::new(), (1, 5));
    lc.record_trusted_batch("a", (0, 20));
    lc.confirm("a", 0.3, None, Vec::new(), (2, 8)).unwrap();
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed: failed 3 of 33 replays this run"
    );
}

#[test]
fn record_trusted_batch_folds_evidence_only_while_trusted() {
    let mut lc = OriginLifecycle::default();
    lc.record_trusted_batch("a", (0, 20));
    assert!(lc.caveat("a").is_none());
    lc.observe("a");
    lc.record_trusted_batch("a", (0, 20));
    assert!(lc.needs_confirmation("a"));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "unconfirmed failure: observed once, never replayed — a rare \
         failure, or the environment changed between executions"
    );
}

#[test]
fn unconfirmed_report_is_sorted_and_skips_confirmed_origins() {
    let mut lc = OriginLifecycle::default();
    assert!(lc.reject("c", (0, 10)));
    assert!(lc.reject("a", (0, 10)));
    assert!(lc.reject("a", (0, 10)));
    lc.observe("b");
    lc.confirm("d", 0.5, None, Vec::new(), (4, 6)).unwrap();
    assert_eq!(lc.unconfirmed().collect::<Vec<_>>(), vec!["a", "b", "c"]);
}

#[test]
fn raise_anchor_is_monotone_and_confirmed_only() {
    let mut lc = OriginLifecycle::default();
    lc.raise_anchor("a", 0.9);
    lc.observe("a");
    lc.raise_anchor("a", 0.9);
    assert!(lc.needs_confirmation("a"));
    lc.trust("b", Vec::new(), (1, 1));
    lc.raise_anchor("b", 0.9);
    assert!(lc.take_witness("b").is_none());
    lc.confirm("c", 0.3, Some(witness("c")), Vec::new(), (4, 12))
        .unwrap();
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
    lc.confirm("a", 0.3, None, Vec::new(), (4, 12)).unwrap();
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed: failed 5 of 62 replays this run"
    );
    lc.trust("b", Vec::new(), (1, 6));
    assert_eq!(
        lc.caveat("b").unwrap(),
        "nondeterministic failure, reproduced from stored timelines: failed \
         1 of 6 replays this run"
    );
}

#[test]
fn a_dry_final_replay_switches_the_confirmed_caveat_wording() {
    let mut lc = OriginLifecycle::default();
    lc.record_final_replay("a", (0, 29));
    assert!(lc.caveat("a").is_none());
    lc.confirm("a", 0.4, None, Vec::new(), (4, 9)).unwrap();
    lc.record_final_replay("a", (0, 29));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed earlier this run (failed 4 of 9 \
         replays) but not reproduced at report time — a rare failure, or \
         something in the environment changed after discovery"
    );
}

#[test]
fn caveats_keep_report_time_counts_apart_from_confirmation_counts() {
    let mut lc = OriginLifecycle::default();
    lc.confirm("a", 0.4, None, Vec::new(), (4, 9)).unwrap();
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed: failed 4 of 9 replays this run"
    );
    lc.record_final_replay("a", (1, 3));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, confirmed: failed 4 of 9 replays at \
         confirmation and 1 of 3 at report time"
    );
}

#[test]
fn record_final_replay_records_report_counts_on_trusted_and_confirmed() {
    let mut lc = OriginLifecycle::default();
    lc.trust("a", Vec::new(), (1, 5));
    lc.record_final_replay("a", (2, 4));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, reproduced from stored timelines: failed \
         1 of 5 replays at reuse and 2 of 4 at report time"
    );
}

#[test]
fn a_dry_final_replay_switches_the_trusted_caveat_wording() {
    let mut lc = OriginLifecycle::default();
    lc.trust("a", Vec::new(), (1, 5));
    lc.record_final_replay("a", (0, 20));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "nondeterministic failure, reproduced from stored timelines earlier \
         this run (failed 1 of 5 replays) but not reproduced at report time \
         — a rare failure, or something in the environment changed after \
         discovery"
    );
}

#[test]
fn trust_truncates_an_oversized_pool_to_pool_cap() {
    let mut lc = OriginLifecycle::default();
    let pool: Vec<Vec<ChoiceValue>> = (0..crate::native::nd::POOL_CAP + 3)
        .map(|i| vec![ChoiceValue::Boolean(i % 2 == 0); i + 1])
        .collect();
    lc.trust("a", pool, (1, 1));
    assert_eq!(lc.pool("a").len(), crate::native::nd::POOL_CAP);
}

#[test]
fn confirm_truncates_an_oversized_pool_to_pool_cap() {
    let mut lc = OriginLifecycle::default();
    let pool: Vec<Vec<ChoiceValue>> = (0..crate::native::nd::POOL_CAP + 3)
        .map(|i| vec![ChoiceValue::Boolean(i % 2 == 0); i + 1])
        .collect();
    lc.confirm("a", 0.4, None, pool, (4, 9)).unwrap();
    assert_eq!(lc.pool("a").len(), crate::native::nd::POOL_CAP);
}

#[test]
fn bar_attempts_are_budgeted_per_origin_per_run() {
    let mut lc = OriginLifecycle::default();
    for _ in 0..crate::native::nd::BAR_ATTEMPTS_PER_RUN {
        assert!(lc.spend_bar_attempt("a"));
    }
    assert!(!lc.spend_bar_attempt("a"));
    assert!(!lc.spend_bar_attempt("a"));
    assert!(lc.spend_bar_attempt("b"), "budgets are per origin");
}

#[test]
fn backtrack_attempts_are_a_separate_budget() {
    let mut lc = OriginLifecycle::default();
    for _ in 0..crate::native::nd::BAR_ATTEMPTS_PER_RUN {
        assert!(lc.spend_bar_attempt("a"));
    }
    assert!(lc.backtrack_attempts_left("a"));
    for _ in 0..crate::native::nd::BACKTRACK_BAR_ATTEMPTS {
        assert!(lc.spend_backtrack_attempt("a"));
    }
    assert!(!lc.backtrack_attempts_left("a"));
    assert!(!lc.spend_backtrack_attempt("a"));
    assert!(lc.backtrack_attempts_left("b"));
}

#[test]
fn a_never_replayed_origin_gets_the_observed_once_caveat() {
    let mut lc = OriginLifecycle::default();
    lc.observe("a");
    assert_eq!(
        lc.caveat("a").unwrap(),
        "unconfirmed failure: observed once, never replayed — a rare \
         failure, or the environment changed between executions"
    );
    lc.reject("a", (0, 24));
    assert_eq!(
        lc.caveat("a").unwrap(),
        "unconfirmed failure: failed 0 of 24 replays after the observed \
         failure — a rare failure, or the environment changed between \
         executions"
    );
}
