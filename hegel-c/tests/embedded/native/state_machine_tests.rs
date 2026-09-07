use super::*;
use crate::native::core::choices::ChoiceTemplate;
use crate::native::core::{ChoiceKind, ChoiceValue, Status};
use crate::native::rng::EngineRng;
use alloc::string::ToString;
use alloc::vec;

fn machine(ntc: &mut NativeTestCase, num_rules: usize) -> NativeStateMachine {
    machine_concurrent(ntc, num_rules, 1)
}

fn machine_concurrent(
    ntc: &mut NativeTestCase,
    num_rules: usize,
    concurrency: i64,
) -> NativeStateMachine {
    NativeStateMachine::new(ntc, vec![0; num_rules], 0, concurrency, concurrency).unwrap()
}

fn grouped_machine(ntc: &mut NativeTestCase, rule_groups: &[i64]) -> NativeStateMachine {
    NativeStateMachine::new(ntc, rule_groups.to_vec(), 0, 1, 1).unwrap()
}

fn replay(prefix: &[ChoiceValue], max_size: usize) -> NativeTestCase {
    NativeTestCase::for_choices_and_template(prefix, None, None, max_size, None)
}

fn simplest_after(prefix: &[ChoiceValue], max_size: usize) -> NativeTestCase {
    NativeTestCase::for_choices_and_template(
        prefix,
        None,
        Some(ChoiceTemplate::simplest(None).unwrap()),
        max_size,
        None,
    )
}

fn int(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

/// A keep-going round decision. The first round's decision is forced, so a
/// prefix entry at its position is a placeholder that only keeps later
/// entries aligned.
fn go() -> ChoiceValue {
    ChoiceValue::Boolean(true)
}

/// A stop round decision: the simplest boolean.
fn stop() -> ChoiceValue {
    ChoiceValue::Boolean(false)
}

/// A simplest-template test case whose prefix keeps a sequential machine
/// with every rule enabled running for `rounds` rounds: the p_disabled
/// draw, then per round the continue decision, the rule index, and the
/// feature-flag placeholder. Draws past the prefix take the simplest value,
/// so the machine stops at the next free round decision.
fn sequential_rounds(rounds: usize, max_size: usize) -> NativeTestCase {
    let mut prefix = vec![int(0)];
    for _ in 0..rounds {
        prefix.extend([go(), int(0), ChoiceValue::Boolean(false)]);
    }
    simplest_after(&prefix, max_size)
}

/// The node recording the rule index chosen by the enumeration fallback:
/// forced, and in the same `[0, n-1]` domain as the rejection-sampling tries.
fn assert_forced_index_node(ntc: &NativeTestCase, pos: usize, n: i64, index: i64) {
    let node = &ntc.nodes[pos];
    assert!(node.was_forced);
    assert_eq!(node.value(), ChoiceValue::Integer(BigInt::from(index)));
    assert!(matches!(&node.kind(), ChoiceKind::Integer(k) if k.max_value == BigInt::from(n - 1)));
}

fn count_draws_with_max(ntc: &NativeTestCase, max_value: i64) -> usize {
    ntc.nodes
        .iter()
        .filter(
            |n| matches!(&n.kind(), ChoiceKind::Integer(k) if k.max_value == BigInt::from(max_value)),
        )
        .count()
}

#[test]
fn first_round_decision_is_a_forced_keep_going_boolean() {
    let mut ntc = replay(&[int(0), stop(), int(2)], 8);
    let mut sm = machine(&mut ntc, 3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(ntc.nodes[1].was_forced);
    assert_eq!(ntc.nodes[1].value(), ChoiceValue::Boolean(true));
}

#[test]
fn zero_p_disabled_enables_every_rule() {
    let mut ntc = replay(&[int(0), go(), int(2)], 8);
    let mut sm = machine(&mut ntc, 3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap();
    assert_eq!(rule, Some(2));
    assert_eq!(ntc.nodes.len(), 4);
    assert!(ntc.nodes[3].was_forced);
    assert_eq!(ntc.nodes[3].value(), ChoiceValue::Boolean(false));
    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(
        ntc.spans[0usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG as u64).to_string()
    );
    assert!(!ntc.spans[0usize].discarded);
}

#[test]
fn bounded_case_runs_at_most_step_count_rounds() {
    let mut ntc = sequential_rounds(6, 4096);
    ntc.family().set_stateful_step_count(5);
    let mut sm = machine(&mut ntc, 2);
    for _ in 0..5 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
    assert!(
        ntc.nodes.last().unwrap().was_forced,
        "the stop at the step count is forced"
    );
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn simplest_template_runs_exactly_one_round() {
    let mut ntc = simplest_after(&[], 4096);
    ntc.family().set_stateful_step_count(500);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
    assert!(
        !ntc.nodes.last().unwrap().was_forced,
        "the stop is the simplest value"
    );
    assert_eq!(
        ntc.nodes.last().unwrap().value(),
        ChoiceValue::Boolean(false)
    );
    assert_eq!(ntc.nodes.len(), 5);
}

#[test]
fn bounded_case_runs_at_least_one_round_even_with_step_count_one() {
    let mut ntc = simplest_after(&[], 64);
    ntc.family().set_stateful_step_count(1);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn free_stop_decision_can_halt_before_the_step_count() {
    let prefix = [int(254), go(), int(0), ChoiceValue::Boolean(false), stop()];
    let mut ntc = replay(&prefix, 16);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn sequential_machine_hands_out_exactly_one_rule_per_round() {
    let mut ntc = sequential_rounds(2, 4096);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(sm.next_rule(&mut ntc, 0).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(sm.next_rule(&mut ntc, 0).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
}

#[test]
fn p_disabled_is_drawn_at_creation_only() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let mut sm = machine(&mut ntc, 3);
    assert_eq!(count_draws_with_max(&ntc, 254), 1);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    sm.next_rule(&mut ntc, 0).unwrap();
    sm.next_group(&mut ntc).unwrap();
    sm.next_rule(&mut ntc, 0).unwrap();
    assert_eq!(count_draws_with_max(&ntc, 254), 1);
}

#[test]
fn last_undecided_rule_is_forced_enabled() {
    let prefix = [int(254), go(), int(0), ChoiceValue::Boolean(true), int(1)];
    let mut ntc = replay(&prefix, 8);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 1);
    assert_eq!(ntc.nodes.len(), 6);
    assert!(ntc.nodes[5].was_forced);
    assert_eq!(ntc.nodes[5].value(), ChoiceValue::Boolean(false));
}

#[test]
fn decided_flag_is_rewritten_as_forced_draw_on_later_queries() {
    let prefix = [
        int(254),
        go(),
        int(0),
        ChoiceValue::Boolean(false),
        go(),
        int(0),
    ];
    let mut ntc = replay(&prefix, 8);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap().unwrap(), 0);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap().unwrap(), 0);
    assert_eq!(ntc.nodes.len(), 7);
    assert!(ntc.nodes[6].was_forced);
    assert_eq!(ntc.nodes[6].value(), ChoiceValue::Boolean(false));
}

#[test]
fn known_disabled_rule_is_skipped_without_redrawing_its_flag() {
    let prefix = [
        int(254),
        go(),
        int(1),
        ChoiceValue::Boolean(true),
        int(1),
        int(2),
        ChoiceValue::Boolean(false),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = machine(&mut ntc, 3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 7);
}

#[test]
fn fallback_early_exits_at_the_speculative_index() {
    let prefix = [
        int(254),
        go(),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = machine(&mut ntc, 3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 10);
    assert_forced_index_node(&ntc, 9, 3, 2);
}

#[test]
fn fallback_draws_from_allowed_when_speculative_index_is_past_the_end() {
    let prefix = [
        int(254),
        go(),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = machine(&mut ntc, 4);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 3);
    assert_eq!(ntc.nodes.len(), 12);
    assert!(ntc.nodes[9].was_forced);
    assert_forced_index_node(&ntc, 11, 4, 3);
}

#[test]
fn next_group_draws_and_returns_the_current_group_when_there_are_several() {
    let prefix = [int(254), go(), int(1), int(0)];
    let mut ntc = replay(&prefix, 8);
    let mut sm = grouped_machine(&mut ntc, &[0, 0, 1]);
    assert_eq!(sm.next_group(&mut ntc).unwrap(), Some(1));
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 5);
    assert!(ntc.nodes[4].was_forced);
    assert_eq!(ntc.nodes[4].value(), ChoiceValue::Boolean(false));
}

#[test]
fn group_ids_are_arbitrary_and_deduplicated_by_first_appearance() {
    let prefix = [int(254), go(), int(1), int(0)];
    let mut ntc = replay(&prefix, 8);
    let mut sm = grouped_machine(&mut ntc, &[7, 7, -3]);
    assert_eq!(sm.next_group(&mut ntc).unwrap(), Some(-3));
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 2);
}

#[test]
fn selection_stays_in_the_current_group() {
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let mut sm = grouped_machine(&mut ntc, &[0, 1, 0, 1, 1]);
        let mut rounds = 0;
        for _ in 0..30 {
            let Some(group) = sm.next_group(&mut ntc).unwrap() else {
                break;
            };
            let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap() as usize;
            assert_eq!([0, 1, 0, 1, 1][rule], group);
            rounds += 1;
        }
        assert!(rounds >= 1, "the first round is always forced to run");
    }
}

#[test]
fn at_least_one_rule_per_group_is_forced_enabled() {
    let prefix = [
        int(254),
        go(),
        int(0),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = grouped_machine(&mut ntc, &[0, 0, 1]);
    assert_eq!(sm.next_group(&mut ntc).unwrap(), Some(0));
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 1);
    assert_eq!(ntc.nodes.len(), 7);
    assert!(ntc.nodes[6].was_forced);
    assert_eq!(ntc.nodes[6].value(), ChoiceValue::Boolean(false));
}

#[test]
fn concurrent_workers_have_their_own_flags_and_round_budgets() {
    let prefix = [
        int(0),
        int(0),
        go(),
        ChoiceValue::Boolean(true),
        int(0),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(true),
        int(0),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(false),
        go(),
        ChoiceValue::Boolean(true),
        int(0),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(true),
        int(0),
        ChoiceValue::Boolean(false),
    ];
    let mut ntc = replay(&prefix, 32);
    let mut sm = machine_concurrent(&mut ntc, 2, 2);
    assert_eq!(count_draws_with_max(&ntc, 254), 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert_eq!(sm.next_rule(&mut ntc, 1).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 1).unwrap(), None);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 1).unwrap(), Some(0));
}

#[test]
fn simplest_template_runs_no_rules() {
    let mut ntc = simplest_after(
        &[int(0), int(0), int(0), go(), stop(), stop(), stop(), go()],
        4096,
    );
    ntc.family().set_stateful_step_count(2);
    let mut sm = machine_concurrent(&mut ntc, 2, 3);
    for _ in 0..2 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        for worker in 0..3 {
            assert_eq!(sm.next_rule(&mut ntc, worker).unwrap(), None);
        }
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn concurrent_worker_continue_decision_is_a_recorded_hazard_boolean() {
    let prefix = [
        int(0),
        int(0),
        go(),
        ChoiceValue::Boolean(true),
        int(0),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(true),
        int(0),
        ChoiceValue::Boolean(false),
        ChoiceValue::Boolean(false),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = machine_concurrent(&mut ntc, 2, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert!(
        !ntc.nodes[3].was_forced,
        "the first continue is a free draw: a worker can run zero rules"
    );
    assert_eq!(ntc.nodes[3].value(), ChoiceValue::Boolean(true));
    assert!(
        !ntc.nodes[6].was_forced,
        "mid-round continues are free draws"
    );
    assert_eq!(ntc.nodes[6].value(), ChoiceValue::Boolean(true));
    assert!(!ntc.nodes[9].was_forced, "the stop is the simplest value");
    assert_eq!(ntc.nodes[9].value(), ChoiceValue::Boolean(false));
}

#[test]
fn concurrent_rounds_stop_at_the_step_count() {
    let mut ntc = simplest_after(
        &[int(0), int(0), int(0), go(), go(), go(), go(), go()],
        4096,
    );
    ntc.family().set_stateful_step_count(4);
    let mut sm = machine_concurrent(&mut ntc, 2, 3);
    for _ in 0..4 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn concurrent_rejections_refund_the_worker_round_budget() {
    let mut ntc = simplest_after(&[int(0), int(0), go(), ChoiceValue::Boolean(true)], 4096);
    let mut sm = machine_concurrent(&mut ntc, 2, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    for _ in 0..10 {
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
        sm.rule_rejected(0).unwrap();
    }
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
}

#[test]
fn concurrent_worker_attempts_are_capped_per_round() {
    let mut ntc = simplest_after(&[int(0), int(0), go(), ChoiceValue::Boolean(true)], 4096);
    let mut sm = machine_concurrent(&mut ntc, 2, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    for _ in 0..(MAX_ROUND_RULES * ATTEMPT_MULTIPLIER) {
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
        sm.rule_rejected(0).unwrap();
    }
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
}

#[test]
fn rejected_rounds_do_not_count_toward_the_round_budget() {
    let mut ntc = sequential_rounds(9, 4096);
    ntc.family().set_stateful_step_count(3);
    let mut sm = machine(&mut ntc, 2);
    for _ in 0..5 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
        sm.rule_rejected(0).unwrap();
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    }
    for _ in 0..3 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn round_attempts_stop_at_ten_times_the_step_count_once_a_round_has_succeeded() {
    let mut ntc = sequential_rounds(21, 4096);
    ntc.family().set_stateful_step_count(2);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    for _ in 0..19 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
        sm.rule_rejected(0).unwrap();
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn a_machine_with_no_successful_rounds_gets_a_thousand_attempts() {
    let mut ntc = sequential_rounds(1001, 16384);
    ntc.family().set_stateful_step_count(2);
    let mut sm = machine(&mut ntc, 2);
    for _ in 0..1000 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
        sm.rule_rejected(0).unwrap();
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn rule_rejected_without_an_outstanding_rule_is_an_error() {
    let mut ntc = sequential_rounds(2, 64);
    ntc.family().set_stateful_step_count(5);
    let mut sm = machine(&mut ntc, 2);
    assert!(matches!(
        sm.rule_rejected(0),
        Err(EngineError::InvalidArgument(_))
    ));
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    sm.rule_rejected(0).unwrap();
    assert!(matches!(
        sm.rule_rejected(0),
        Err(EngineError::InvalidArgument(_))
    ));
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    sm.rule_rejected(0).unwrap();
}

#[test]
fn rule_rejected_for_an_out_of_range_worker_is_an_error() {
    let mut ntc = simplest_after(&[int(0), int(0), go(), ChoiceValue::Boolean(true)], 64);
    let mut sm = machine_concurrent(&mut ntc, 2, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert!(matches!(
        sm.rule_rejected(2),
        Err(EngineError::InvalidArgument(_))
    ));
    assert!(matches!(
        sm.rule_rejected(-1),
        Err(EngineError::InvalidArgument(_))
    ));
}

#[test]
fn a_rule_outstanding_at_the_join_point_is_not_rejectable_next_round() {
    let mut ntc = sequential_rounds(2, 64);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.rule_rejected(0),
        Err(EngineError::InvalidArgument(_))
    ));
}

#[test]
fn fixed_concurrency_bounds_consume_no_entropy() {
    let mut ntc = replay(&[int(0), int(0), int(0)], 8);
    let sm = NativeStateMachine::new(&mut ntc, vec![0], 0, 3, 3).unwrap();
    assert_eq!(sm.concurrency(), 3);
    assert_eq!(ntc.nodes.len(), 3);
}

#[test]
fn concurrency_draw_is_max_when_the_weighted_choice_hits() {
    let prefix = [ChoiceValue::Boolean(true), int(0), int(0), int(0), int(0)];
    let mut ntc = replay(&prefix, 8);
    let sm = NativeStateMachine::new(&mut ntc, vec![0], 0, 1, 4).unwrap();
    assert_eq!(sm.concurrency(), 4);
    assert_eq!(
        ntc.spans[0usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_CONCURRENCY as u64).to_string()
    );
}

#[test]
fn concurrency_draw_falls_back_to_a_uniform_level() {
    let prefix = [ChoiceValue::Boolean(false), int(2), int(0), int(0)];
    let mut ntc = replay(&prefix, 8);
    let sm = NativeStateMachine::new(&mut ntc, vec![0], 0, 1, 4).unwrap();
    assert_eq!(sm.concurrency(), 2);
}

#[test]
fn drawn_concurrency_respects_bounds() {
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let sm = NativeStateMachine::new(&mut ntc, vec![0], 0, 2, 5).unwrap();
        assert!((2..=5).contains(&sm.concurrency()));
    }
}

#[test]
fn overrun_while_drawing_the_concurrency_level_propagates() {
    let mut ntc = replay(&[], 0);
    assert!(matches!(
        NativeStateMachine::new(&mut ntc, vec![0], 0, 1, 4),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn next_rule_before_next_group_is_an_invalid_argument() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let mut sm = machine(&mut ntc, 2);
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::InvalidArgument(_))
    ));
}

#[test]
fn out_of_range_worker_index_is_an_invalid_argument() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let mut sm = machine_concurrent(&mut ntc, 2, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 2),
        Err(EngineError::InvalidArgument(_))
    ));
    assert!(matches!(
        sm.next_rule(&mut ntc, -1),
        Err(EngineError::InvalidArgument(_))
    ));
}

fn try_machine(
    ntc: &mut NativeTestCase,
    num_rules: usize,
) -> Result<NativeStateMachine, EngineError> {
    NativeStateMachine::new(ntc, vec![0; num_rules], 0, 1, 1)
}

#[test]
fn overrun_while_drawing_p_disabled_at_creation_propagates() {
    let mut ntc = replay(&[], 0);
    assert!(matches!(
        try_machine(&mut ntc, 2),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_drawing_the_round_decision_propagates() {
    let mut ntc = replay(&[int(0)], 1);
    let mut sm = machine(&mut ntc, 2);
    assert!(matches!(sm.next_group(&mut ntc), Err(EngineError::Overrun)));
}

#[test]
fn overrun_while_drawing_a_try_index_propagates() {
    let mut ntc = replay(&[int(0), go()], 2);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_drawing_a_group_index_propagates() {
    let prefix = [int(254), go()];
    let mut ntc = replay(&prefix, 2);
    let mut sm = grouped_machine(&mut ntc, &[0, 1]);
    assert!(matches!(sm.next_group(&mut ntc), Err(EngineError::Overrun)));
}

#[test]
fn overrun_while_recording_the_early_exit_index_propagates() {
    let prefix = [
        int(254),
        go(),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 9);
    let mut sm = machine(&mut ntc, 3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_recording_the_post_loop_index_propagates() {
    let prefix = [
        int(254),
        go(),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 11);
    let mut sm = machine(&mut ntc, 4);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_inside_is_enabled_leaves_the_span_open_until_freeze() {
    let mut ntc = replay(&[int(254), go(), int(0)], 3);
    let mut sm = machine(&mut ntc, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
    assert_eq!(ntc.status(), Some(Status::EarlyStop));
    ntc.freeze();
    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(
        ntc.spans[0usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG as u64).to_string()
    );
    assert_eq!(ntc.spans[0usize].start, 3);
    assert_eq!(ntc.spans[0usize].end, 3);
}

#[test]
fn all_selected_rules_are_in_range() {
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let mut sm = machine(&mut ntc, 5);
        let mut rounds = 0;
        for _ in 0..30 {
            if sm.next_group(&mut ntc).unwrap().is_none() {
                break;
            }
            assert!(sm.next_rule(&mut ntc, 0).unwrap().unwrap() < 5);
            rounds += 1;
        }
        assert!(rounds >= 1, "the first round is always forced to run");
    }
}

#[test]
fn simplest_template_always_selects_rule_zero() {
    let mut ntc = sequential_rounds(5, 64);
    let mut sm = machine(&mut ntc, 3);
    for _ in 0..5 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap().unwrap(), 0);
    }
}

#[test]
#[should_panic(expected = "Stateful testing: there must be at least one rule")]
fn no_rules_is_error() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    try_machine(&mut ntc, 0).unwrap();
}

#[test]
#[should_panic(expected = "Stateful testing: concurrency bounds must satisfy 1 <= min <= max")]
fn zero_min_concurrency_is_error() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    NativeStateMachine::new(&mut ntc, vec![0], 0, 0, 1).unwrap();
}

#[test]
#[should_panic(expected = "Stateful testing: concurrency bounds must satisfy 1 <= min <= max")]
fn inverted_concurrency_bounds_is_error() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    NativeStateMachine::new(&mut ntc, vec![0], 0, 2, 1).unwrap();
}

#[test]
fn should_check_invariant_rejects_out_of_range_indices() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let mut sm = NativeStateMachine::new(&mut ntc, vec![0], 2, 1, 1).unwrap();
    assert!(matches!(
        sm.should_check_invariant(&mut ntc, 2),
        Err(EngineError::InvalidArgument(_))
    ));
    assert!(matches!(
        sm.should_check_invariant(&mut ntc, -1),
        Err(EngineError::InvalidArgument(_))
    ));
}

#[test]
fn should_check_invariant_is_always_true_at_step_count_one() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    ntc.family().set_stateful_step_count(1);
    let mut sm = NativeStateMachine::new(&mut ntc, vec![0], 1, 1, 1).unwrap();
    for _ in 0..10 {
        assert!(sm.should_check_invariant(&mut ntc, 0).unwrap());
    }
}

#[test]
fn should_check_invariant_samples_at_one_over_step_count() {
    let mut trues = 0;
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        ntc.family().set_stateful_step_count(50);
        let mut sm = NativeStateMachine::new(&mut ntc, vec![0], 1, 1, 1).unwrap();
        for _ in 0..100 {
            if sm.should_check_invariant(&mut ntc, 0).unwrap() {
                trues += 1;
            }
        }
    }
    assert!(
        (10..=100).contains(&trues),
        "expected about 40 sampled checks in 2000, got {trues}"
    );
}
