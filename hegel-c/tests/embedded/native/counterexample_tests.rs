//! Embedded tests for `src/native/counterexample.rs`.

use super::*;
use crate::native::HashMap;
use crate::native::bignum::BigInt;
use crate::native::core::Status;
use alloc::vec;
use alloc::vec::Vec;

fn witness(origin: &str) -> RunResult {
    RunResult {
        status: Status::Interesting,
        nodes: Vec::new(),
        spans: Vec::new(),
        origin: Some(String::from(origin)),
        target_observations: HashMap::default(),
        events: Vec::new(),
    }
}

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        crate::native::core::choices::IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(100),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

fn values(nodes: &[ChoiceNode]) -> Vec<ChoiceValue> {
    nodes.iter().map(|n| n.value()).collect()
}

#[test]
fn a_blank_counterexample_needs_confirmation_and_holds_nothing() {
    let mut c = Counterexample::default();
    assert!(c.needs_confirmation());
    assert!(c.incumbent().is_none());
    assert!(c.take_witness().is_none());
    assert!(c.pool().is_empty());
    assert!(c.timelines().is_empty());
    assert!(c.history().is_empty());
    assert!(!c.first_checked());
    assert!(c.reject((0, 10)).is_none(), "nothing to evict");
}

#[test]
fn adopt_founds_then_only_shortlex_displaces() {
    let mut c = Counterexample::default();
    assert!(c.adopt(vec![int_node(5), int_node(5)]));
    assert!(!c.adopt(vec![int_node(9), int_node(9)]), "shortlex-larger");
    assert!(c.adopt(vec![int_node(7)]), "shorter wins");
    assert_eq!(c.incumbent().unwrap(), &[int_node(7)]);
    c.replace(vec![int_node(100), int_node(100)]);
    assert_eq!(c.incumbent().unwrap().len(), 2, "replace is unconditional");
}

#[test]
fn rejection_evicts_the_incumbent_but_keeps_the_evidence() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)]);
    assert_eq!(c.reject((1, 10)), Some(vec![int_node(1)]));
    assert!(c.incumbent().is_none());
    assert!(c.adopt(vec![int_node(2)]), "a re-sighting founds again");
    assert_eq!(c.reject((2, 30)), Some(vec![int_node(2)]));
    assert_eq!(
        c.caveat(),
        "unconfirmed failure: failed 3 of 40 replays this run, below the \
         confirmation bar — likely rare"
    );
}

#[test]
fn confirmation_stores_replay_state_and_the_witness_is_taken_once() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)]);
    c.confirm(0.4, Some(witness("a")), vec![Vec::new()], (4, 9))
        .unwrap();
    assert!(!c.needs_confirmation());
    assert_eq!(c.pool().len(), 1);
    let (run, anchor) = c.take_witness().unwrap();
    assert_eq!(run.origin.as_deref(), Some("a"));
    assert_eq!(anchor, 0.4);
    assert!(c.take_witness().is_none());
    assert!(c.reject((0, 10)).is_none(), "confirmed origins are exempt");
    assert!(c.incumbent().is_some());
}

#[test]
fn confirmation_drops_the_history() {
    let mut c = Counterexample::default();
    c.record_sighting(&[int_node(3)], true).unwrap();
    c.record_sighting(&[int_node(3)], false).unwrap();
    c.record_sighting(&[int_node(4)], false).unwrap();
    assert_eq!(c.history().entries().len(), 2, "deduplicated by choices");
    assert!(c.history().entries()[0].accept);
    c.confirm(0.4, None, Vec::new(), (4, 9)).unwrap();
    assert!(c.history().is_empty());
}

#[test]
fn timelines_put_the_current_incumbent_ahead_of_the_captured_pool() {
    let mut c = Counterexample::default();
    let confirmed = vec![int_node(9), int_node(9)];
    c.adopt(confirmed.clone());
    c.confirm(
        0.5,
        None,
        pooled_timelines(values(&confirmed), vec![values(&[int_node(7)])]),
        (4, 9),
    )
    .unwrap();
    c.replace(vec![int_node(1)]);
    let timelines = c.timelines();
    assert_eq!(timelines[0], values(&[int_node(1)]));
    assert_eq!(timelines[1], values(&confirmed));
    assert_eq!(timelines[2], values(&[int_node(7)]));
    let state = c.repro_state(values(&[int_node(1)])).unwrap();
    assert_eq!(state.timelines, timelines);
    assert_eq!(
        state.extension as usize,
        crate::native::nd::continuation_budget(1) - 1
    );
    assert_eq!(c.timelines_from(values(&confirmed)).len(), 2);
}

#[test]
fn pooled_timelines_deduplicates_and_caps_incumbent_included() {
    let rest: Vec<Vec<ChoiceValue>> = (0..crate::native::nd::POOL_CAP + 3)
        .map(|i| vec![ChoiceValue::Boolean(i % 2 == 0); i + 1])
        .collect();
    let pool = pooled_timelines(rest[0].clone(), rest.clone());
    assert_eq!(pool.len(), crate::native::nd::POOL_CAP);
    assert_eq!(pool[0], rest[0]);
    assert_eq!(
        pool[1], rest[1],
        "the duplicate of the incumbent is skipped"
    );
}

#[test]
fn trusted_origins_survive_rejection_without_eviction() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)]);
    c.trust(Vec::new(), (1, 2));
    assert!(!c.needs_confirmation());
    assert!(c.reject((0, 10)).is_none());
    assert!(c.incumbent().is_some());
    assert!(c.take_witness().is_none());
}

#[test]
fn trust_carries_a_stored_pool_and_never_replaces_it_with_an_empty_one() {
    let mut c = Counterexample::default();
    c.trust(vec![vec![ChoiceValue::Boolean(true)]], (1, 1));
    assert_eq!(c.pool().len(), 1);
    c.trust(Vec::new(), (1, 1));
    assert_eq!(c.pool().len(), 1);
    c.trust(vec![Vec::new(), vec![ChoiceValue::Boolean(false)]], (1, 1));
    assert_eq!(c.pool().len(), 2);
}

#[test]
fn trust_seeds_and_folds_reuse_evidence() {
    let mut c = Counterexample::default();
    c.trust(Vec::new(), (1, 4));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored timelines: failed \
         1 of 4 replays this run"
    );
    c.trust(Vec::new(), (2, 3));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored timelines: failed \
         3 of 7 replays this run"
    );
    assert!(!c.needs_confirmation());
}

#[test]
fn trust_never_demotes_a_confirmed_origin() {
    let mut c = Counterexample::default();
    c.confirm(0.7, Some(witness("a")), Vec::new(), (4, 4))
        .unwrap();
    c.trust(vec![Vec::new()], (1, 1));
    let (_, anchor) = c.take_witness().unwrap();
    assert_eq!(anchor, 0.7);
    assert!(c.pool().is_empty());
}

#[test]
fn confirm_on_a_confirmed_origin_is_an_internal_error() {
    let mut c = Counterexample::default();
    c.confirm(0.4, None, Vec::new(), (4, 9)).unwrap();
    assert!(c.confirm(0.5, None, Vec::new(), (4, 4)).is_err());
}

#[test]
fn promotion_folds_trusted_evidence_into_the_confirmed_counts() {
    let mut c = Counterexample::default();
    c.trust(Vec::new(), (1, 5));
    c.record_trusted_batch((0, 20));
    c.confirm(0.3, None, Vec::new(), (2, 8)).unwrap();
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, confirmed: failed 3 of 33 replays this run"
    );
}

#[test]
fn record_trusted_batch_folds_evidence_only_while_trusted() {
    let mut c = Counterexample::default();
    c.record_trusted_batch((0, 20));
    assert!(c.needs_confirmation());
    assert_eq!(
        c.caveat(),
        "unconfirmed failure: observed once, never replayed — a rare \
         failure, or the environment changed between executions"
    );
}

#[test]
fn raise_anchor_is_monotone_and_confirmed_only() {
    let mut c = Counterexample::default();
    c.raise_anchor(0.9);
    assert!(c.needs_confirmation());
    let mut t = Counterexample::default();
    t.trust(Vec::new(), (1, 1));
    t.raise_anchor(0.9);
    assert!(t.take_witness().is_none());
    let mut k = Counterexample::default();
    k.confirm(0.3, Some(witness("c")), Vec::new(), (4, 12))
        .unwrap();
    k.raise_anchor(0.2);
    k.raise_anchor(0.6);
    let (_, anchor) = k.take_witness().unwrap();
    assert_eq!(anchor, 0.6);
}

#[test]
fn caveats_quote_the_accumulated_replay_evidence() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)]);
    assert!(c.reject((1, 40)).is_some());
    assert_eq!(
        c.caveat(),
        "unconfirmed failure: failed 1 of 40 replays this run, below the \
         confirmation bar — likely rare"
    );
    c.reject((0, 10));
    assert_eq!(
        c.caveat(),
        "unconfirmed failure: failed 1 of 50 replays this run, below the \
         confirmation bar — likely rare"
    );
    c.confirm(0.3, None, Vec::new(), (4, 12)).unwrap();
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, confirmed: failed 5 of 62 replays this run"
    );
}

#[test]
fn a_dry_final_replay_switches_the_confirmed_caveat_wording() {
    let mut c = Counterexample::default();
    c.record_final_replay((0, 29));
    assert!(c.needs_confirmation(), "no-op while unconfirmed");
    c.confirm(0.4, None, Vec::new(), (4, 9)).unwrap();
    c.record_final_replay((0, 29));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, confirmed earlier this run (failed 4 of 9 \
         replays) but not reproduced at report time — a rare failure, or \
         something in the environment changed after discovery"
    );
}

#[test]
fn caveats_keep_report_time_counts_apart_from_confirmation_counts() {
    let mut c = Counterexample::default();
    c.confirm(0.4, None, Vec::new(), (4, 9)).unwrap();
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, confirmed: failed 4 of 9 replays this run"
    );
    c.record_final_replay((1, 3));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, confirmed: failed 4 of 9 replays at \
         confirmation and 1 of 3 at report time"
    );
}

#[test]
fn record_final_replay_records_report_counts_on_trusted() {
    let mut c = Counterexample::default();
    c.trust(Vec::new(), (1, 5));
    c.record_final_replay((2, 4));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored timelines: failed \
         1 of 5 replays at reuse and 2 of 4 at report time"
    );
}

#[test]
fn a_dry_final_replay_switches_the_trusted_caveat_wording() {
    let mut c = Counterexample::default();
    c.trust(Vec::new(), (1, 5));
    c.record_final_replay((0, 20));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored timelines earlier \
         this run (failed 1 of 5 replays) but not reproduced at report time \
         — a rare failure, or something in the environment changed after \
         discovery"
    );
}

#[test]
fn trust_and_confirm_truncate_an_oversized_pool_to_pool_cap() {
    let pool: Vec<Vec<ChoiceValue>> = (0..crate::native::nd::POOL_CAP + 3)
        .map(|i| vec![ChoiceValue::Boolean(i % 2 == 0); i + 1])
        .collect();
    let mut t = Counterexample::default();
    t.trust(pool.clone(), (1, 1));
    assert_eq!(t.pool().len(), crate::native::nd::POOL_CAP);
    let mut c = Counterexample::default();
    c.confirm(0.4, None, pool, (4, 9)).unwrap();
    assert_eq!(c.pool().len(), crate::native::nd::POOL_CAP);
}

#[test]
fn bar_attempts_are_budgeted_per_run() {
    let mut c = Counterexample::default();
    for _ in 0..crate::native::nd::BAR_ATTEMPTS_PER_RUN {
        assert!(c.spend_bar_attempt());
    }
    assert!(!c.spend_bar_attempt());
    assert!(!c.spend_bar_attempt());
}

#[test]
fn backtrack_attempts_are_a_separate_budget() {
    let mut c = Counterexample::default();
    for _ in 0..crate::native::nd::BAR_ATTEMPTS_PER_RUN {
        assert!(c.spend_bar_attempt());
    }
    assert!(c.backtrack_attempts_left());
    for _ in 0..crate::native::nd::BACKTRACK_BAR_ATTEMPTS {
        assert!(c.spend_backtrack_attempt());
    }
    assert!(!c.backtrack_attempts_left());
    assert!(!c.spend_backtrack_attempt());
}

#[test]
fn a_never_replayed_origin_gets_the_observed_once_caveat() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)]);
    assert_eq!(
        c.caveat(),
        "unconfirmed failure: observed once, never replayed — a rare \
         failure, or the environment changed between executions"
    );
    c.reject((0, 24));
    assert_eq!(
        c.caveat(),
        "unconfirmed failure: failed 0 of 24 replays after the observed \
         failure — a rare failure, or the environment changed between \
         executions"
    );
}

#[test]
fn seeds_are_taken_once_and_first_check_is_sticky() {
    let mut c = Counterexample::default();
    assert!(c.take_seed().is_none());
    let mut seed = Evidence::default();
    seed.record(true);
    c.seed_evidence(seed);
    assert_eq!(c.take_seed().unwrap().runs(), 1);
    assert!(c.take_seed().is_none());
    c.mark_first_checked();
    assert!(c.first_checked());
}

#[test]
fn the_map_reports_live_and_unconfirmed_origins_in_origin_order() {
    let mut all = Counterexamples::default();
    assert!(!all.any_live());
    assert!(all.needs_confirmation("zeta"), "unknown origins do");
    assert!(all.caveat("zeta").is_none());
    all.entry("c").adopt(vec![int_node(1)]);
    all.entry("a").adopt(vec![int_node(2)]);
    all.entry("b")
        .confirm(0.5, None, Vec::new(), (4, 6))
        .unwrap();
    assert!(all.any_live());
    assert_eq!(all.live_origins(), vec!["a", "c"]);
    assert_eq!(all.unconfirmed().collect::<Vec<_>>(), vec!["a", "c"]);
    assert!(all.entry("c").reject((0, 10)).is_some());
    assert_eq!(all.live_origins(), vec!["a"], "evicted but still known");
    assert_eq!(all.unconfirmed().collect::<Vec<_>>(), vec!["a", "c"]);
    assert_eq!(all.incumbent("a").unwrap(), &[int_node(2)]);
    assert!(all.incumbent("c").is_none());
    assert!(all.caveat("c").is_some());
    assert_eq!(all.iter().count(), 3);
}
