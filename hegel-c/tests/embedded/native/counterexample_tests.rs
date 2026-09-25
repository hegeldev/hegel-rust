//! Embedded tests for `src/native/counterexample.rs`.

use super::*;
use crate::native::HashMap;
use crate::native::bignum::BigInt;
use crate::native::core::{ChoiceValue, Status};
use crate::native::graph::{DRAW_LABEL, Step, Walked};
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
        divergence: None,
        settled: Vec::new(),
        ended: false,
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

fn span(label: u64, start: usize, end: usize) -> Span {
    Span {
        label,
        start,
        end,
        depth: 0,
        parent: None,
        discarded: false,
    }
}

fn graph_of(values: &[i128]) -> Graph {
    Graph::from_run(&Run {
        steps: values
            .iter()
            .map(|&v| Step {
                addr: Vec::new(),
                value: ChoiceValue::Integer(BigInt::from(v)),
            })
            .collect(),
    })
}

#[test]
fn a_blank_counterexample_needs_confirmation_and_holds_nothing() {
    let mut c = Counterexample::default();
    assert!(c.needs_confirmation());
    assert!(c.incumbent().is_none());
    assert!(c.incumbent_spans().is_empty());
    assert!(c.incumbent_run().is_none());
    assert!(c.take_witness().is_none());
    assert!(c.graph().is_none());
    assert!(c.replay_graph().is_none());
    assert_eq!(c.longest(), 0);
    assert!(c.history().is_empty());
    assert!(!c.first_checked());
    assert!(c.reject((0, 10)).is_none(), "nothing to evict");
    assert!(c.repro_state().is_err(), "nothing to store");
}

#[test]
fn adopt_founds_then_only_shortlex_displaces() {
    let mut c = Counterexample::default();
    assert!(c.adopt(vec![int_node(5), int_node(5)], vec![span(3, 0, 2)]));
    assert_eq!(c.incumbent_spans().len(), 1);
    assert!(
        !c.adopt(vec![int_node(9), int_node(9)], Vec::new()),
        "shortlex-larger"
    );
    assert_eq!(
        c.incumbent_spans().len(),
        1,
        "a rejected adopt changes nothing"
    );
    assert!(c.adopt(vec![int_node(7)], Vec::new()), "shorter wins");
    assert_eq!(c.incumbent().unwrap(), &[int_node(7)]);
    assert!(c.incumbent_spans().is_empty());
    c.replace(vec![int_node(100), int_node(100)], vec![span(3, 0, 2)]);
    assert_eq!(c.incumbent().unwrap().len(), 2, "replace is unconditional");
    assert_eq!(c.incumbent_spans().len(), 1);
}

#[test]
fn the_incumbent_run_addresses_its_draws_by_the_spans() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1), int_node(2)], vec![span(7, 1, 2)]);
    let run = c.incumbent_run().unwrap();
    assert_eq!(run.steps[0].addr, vec![(DRAW_LABEL, 0)]);
    assert_eq!(run.steps[1].addr, vec![(7, 0), (DRAW_LABEL, 0)]);
}

#[test]
fn rejection_evicts_the_incumbent_but_keeps_the_evidence() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)], vec![span(3, 0, 1)]);
    assert_eq!(c.reject((1, 10)), Some(vec![int_node(1)]));
    assert!(c.incumbent().is_none());
    assert!(c.incumbent_spans().is_empty(), "the spans go with it");
    assert!(
        c.adopt(vec![int_node(2)], Vec::new()),
        "a re-sighting founds again"
    );
    assert_eq!(c.reject((2, 30)), Some(vec![int_node(2)]));
    assert_eq!(
        c.caveat(),
        "unconfirmed failure: failed 3 of 40 replays this run, below the \
         confirmation bar — likely rare"
    );
}

#[test]
fn an_unconfirmed_origin_replays_as_its_incumbents_run() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1), int_node(2)], Vec::new());
    assert!(c.graph().is_none());
    let graph = c.replay_graph().unwrap();
    assert_eq!(graph.edge_count(), 2);
    assert_eq!(
        graph.walk_verdict(&c.incumbent_run().unwrap()),
        Walked::Whole
    );
    assert_eq!(c.longest(), 2, "floored at the incumbent");
}

#[test]
fn confirmation_stores_replay_state_and_the_witness_is_taken_once() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)], Vec::new());
    c.confirm(0.4, Some(witness("a")), graph_of(&[1, 2, 3]), 3, (4, 9))
        .unwrap();
    assert!(!c.needs_confirmation());
    assert_eq!(c.graph().unwrap().edge_count(), 3);
    assert_eq!(c.replay_graph().unwrap().edge_count(), 3);
    assert_eq!(c.longest(), 3);
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
    c.record_sighting(&[int_node(3)], &[span(5, 0, 1)], true)
        .unwrap();
    c.record_sighting(&[int_node(3)], &[], false).unwrap();
    c.record_sighting(&[int_node(4)], &[], false).unwrap();
    let entries = c.history().entries();
    assert_eq!(entries.len(), 2, "deduplicated by choices");
    assert!(entries[0].accept);
    assert_eq!(
        entries[0].run().steps[0].addr,
        vec![(5, 0), (DRAW_LABEL, 0)]
    );
    assert_eq!(entries[1].run().steps[0].addr, vec![(DRAW_LABEL, 0)]);
    c.confirm(0.4, None, graph_of(&[3]), 1, (4, 9)).unwrap();
    assert!(c.history().is_empty());
}

#[test]
fn longest_is_the_stored_length_floored_at_the_incumbent() {
    let mut c = Counterexample::default();
    c.confirm(0.4, None, graph_of(&[1, 2]), 2, (4, 9)).unwrap();
    assert_eq!(c.longest(), 2);
    c.replace(vec![int_node(1); 5], Vec::new());
    assert_eq!(c.longest(), 5, "a longer incumbent raises the floor");
    c.replace(vec![int_node(1)], Vec::new());
    assert_eq!(c.longest(), 2, "a shorter one leaves the stored length");
}

#[test]
fn repro_state_carries_the_replay_graph_its_hash_and_the_longest_run() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1), int_node(2)], Vec::new());
    let unconfirmed = c.repro_state().unwrap();
    assert_eq!(unconfirmed.graph.edge_count(), 2);
    assert_eq!(unconfirmed.longest, 2);
    assert_eq!(
        unconfirmed.entropy,
        fnv1a(&c.replay_graph().unwrap().encode().unwrap())
    );
    c.confirm(0.4, None, graph_of(&[1, 2, 3, 4]), 4, (4, 9))
        .unwrap();
    let confirmed = c.repro_state().unwrap();
    assert_eq!(confirmed.graph.edge_count(), 4);
    assert_eq!(confirmed.longest, 4);
    assert_ne!(confirmed.entropy, unconfirmed.entropy);
    assert_eq!(
        c.repro_state().unwrap().entropy,
        confirmed.entropy,
        "identical state re-encodes identically"
    );
}

#[test]
fn trusted_origins_survive_rejection_without_eviction() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)], Vec::new());
    c.trust(None, (1, 2));
    assert!(!c.needs_confirmation());
    assert!(c.reject((0, 10)).is_none());
    assert!(c.incumbent().is_some());
    assert!(c.take_witness().is_none());
    assert!(c.graph().is_none(), "a version-1 entry carries no graph");
    assert_eq!(
        c.replay_graph().unwrap().edge_count(),
        1,
        "so the origin replays as its incumbent"
    );
}

#[test]
fn trust_carries_a_stored_graph_and_never_replaces_it_with_nothing() {
    let mut c = Counterexample::default();
    c.trust(Some((Arc::new(graph_of(&[1, 2])), 2)), (1, 1));
    assert_eq!(c.graph().unwrap().edge_count(), 2);
    assert_eq!(c.longest(), 2);
    c.trust(None, (1, 1));
    assert_eq!(c.graph().unwrap().edge_count(), 2);
    c.trust(Some((Arc::new(graph_of(&[1, 2, 3])), 3)), (1, 1));
    assert_eq!(c.graph().unwrap().edge_count(), 3);
    assert_eq!(c.longest(), 3);
}

#[test]
fn trust_seeds_and_folds_reuse_evidence() {
    let mut c = Counterexample::default();
    c.trust(None, (1, 4));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored state: failed \
         1 of 4 replays this run"
    );
    c.trust(None, (2, 3));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored state: failed \
         3 of 7 replays this run"
    );
    assert!(!c.needs_confirmation());
}

#[test]
fn trust_never_demotes_a_confirmed_origin() {
    let mut c = Counterexample::default();
    c.confirm(0.7, Some(witness("a")), graph_of(&[1]), 1, (4, 4))
        .unwrap();
    c.trust(Some((Arc::new(graph_of(&[1, 2])), 2)), (1, 1));
    let (_, anchor) = c.take_witness().unwrap();
    assert_eq!(anchor, 0.7);
    assert_eq!(c.graph().unwrap().edge_count(), 1);
    assert_eq!(c.longest(), 1);
}

#[test]
fn confirm_on_a_confirmed_origin_is_an_internal_error() {
    let mut c = Counterexample::default();
    c.confirm(0.4, None, graph_of(&[1]), 1, (4, 9)).unwrap();
    assert!(c.confirm(0.5, None, graph_of(&[1]), 1, (4, 4)).is_err());
}

#[test]
fn promotion_folds_trusted_evidence_into_the_confirmed_counts() {
    let mut c = Counterexample::default();
    c.trust(Some((Arc::new(graph_of(&[1])), 1)), (1, 5));
    c.record_trusted_batch((0, 20));
    c.confirm(0.3, None, graph_of(&[1, 2]), 2, (2, 8)).unwrap();
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, confirmed: failed 3 of 33 replays this run"
    );
    assert_eq!(
        c.graph().unwrap().edge_count(),
        2,
        "the batch's graph replaces the stored one"
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
    assert_eq!(c.anchor(), None);
    let mut t = Counterexample::default();
    t.trust(None, (1, 1));
    t.raise_anchor(0.9);
    assert!(t.take_witness().is_none());
    assert_eq!(t.anchor(), None);
    let mut k = Counterexample::default();
    k.confirm(0.3, Some(witness("c")), graph_of(&[1]), 1, (4, 12))
        .unwrap();
    k.raise_anchor(0.2);
    k.raise_anchor(0.6);
    let (_, anchor) = k.take_witness().unwrap();
    assert_eq!(anchor, 0.6);
    assert_eq!(k.anchor(), Some(0.6));
}

#[test]
fn install_moves_the_whole_counterexample_and_raises_the_anchor() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(9), int_node(9)], Vec::new());
    c.confirm(0.6, None, graph_of(&[9, 9]), 2, (4, 4)).unwrap();
    c.install(
        graph_of(&[3]),
        vec![int_node(3)],
        vec![span(2, 0, 1)],
        0.4,
        1,
    );
    assert_eq!(c.incumbent().unwrap(), &[int_node(3)]);
    assert_eq!(c.incumbent_spans().len(), 1);
    assert_eq!(c.graph().unwrap().edge_count(), 1);
    assert_eq!(c.longest(), 1);
    assert_eq!(c.anchor(), Some(0.6), "never lowered");
    c.install(graph_of(&[2]), vec![int_node(2)], Vec::new(), 0.8, 1);
    assert_eq!(c.anchor(), Some(0.8));
}

#[test]
fn caveats_quote_the_accumulated_replay_evidence() {
    let mut c = Counterexample::default();
    c.adopt(vec![int_node(1)], Vec::new());
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
    c.confirm(0.3, None, graph_of(&[1]), 1, (4, 12)).unwrap();
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
    c.confirm(0.4, None, graph_of(&[1]), 1, (4, 9)).unwrap();
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
    c.confirm(0.4, None, graph_of(&[1]), 1, (4, 9)).unwrap();
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
    c.trust(None, (1, 5));
    c.record_final_replay((2, 4));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored state: failed \
         1 of 5 replays at reuse and 2 of 4 at report time"
    );
}

#[test]
fn a_dry_final_replay_switches_the_trusted_caveat_wording() {
    let mut c = Counterexample::default();
    c.trust(None, (1, 5));
    c.record_final_replay((0, 20));
    assert_eq!(
        c.caveat(),
        "nondeterministic failure, reproduced from stored state earlier \
         this run (failed 1 of 5 replays) but not reproduced at report time \
         — a rare failure, or something in the environment changed after \
         discovery"
    );
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
    c.adopt(vec![int_node(1)], Vec::new());
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
fn clear_history_drops_the_entries_without_confirming() {
    let mut c = Counterexample::default();
    c.record_sighting(&[int_node(3)], &[], true).unwrap();
    c.clear_history();
    assert!(c.history().is_empty());
    assert!(c.needs_confirmation());
}

#[test]
fn the_map_reports_live_and_unconfirmed_origins_in_origin_order() {
    let mut all = Counterexamples::default();
    assert!(!all.any_live());
    assert!(all.needs_confirmation("zeta"), "unknown origins do");
    assert!(all.caveat("zeta").is_none());
    all.entry("c").adopt(vec![int_node(1)], Vec::new());
    all.entry("a").adopt(vec![int_node(2)], Vec::new());
    all.entry("b")
        .confirm(0.5, None, graph_of(&[1]), 1, (4, 6))
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
    assert!(all.get_mut("b").unwrap().anchor().is_some());
    assert_eq!(all.iter_mut().count(), 3);
}

#[test]
fn the_history_thins_raws_first_then_old_accepts_once_over_its_byte_bound() {
    let mut c = Counterexample::default();
    let big = 100_000;
    let mut record = |first: i128, accept: bool| {
        let mut nodes = vec![int_node(0); big];
        nodes[0] = int_node(first);
        c.record_sighting(&nodes, &[span(1, 0, big)], accept)
            .unwrap();
    };
    for (first, accept) in [
        (1, true),
        (2, false),
        (3, false),
        (4, true),
        (5, true),
        (6, true),
        (7, true),
        (8, true),
    ] {
        record(first, accept);
    }
    let firsts: Vec<ChoiceValue> = c
        .history()
        .entries()
        .iter()
        .map(|e| e.nodes[0].value())
        .collect();
    let expected: Vec<ChoiceValue> = [1, 5, 6, 7, 8]
        .into_iter()
        .map(|v| ChoiceValue::Integer(BigInt::from(v)))
        .collect();
    assert_eq!(
        firsts, expected,
        "raws 2 and 3 go first, then the second-oldest accept 4"
    );
    assert!(c.history().bytes() <= HISTORY_BYTES);
    assert!(c.history().entries().iter().all(|e| e.accept));
}
