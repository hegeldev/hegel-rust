use super::*;
use crate::native::core::choices::ChoiceTemplate;
use crate::native::core::{ChoiceKind, ChoiceValue, Status};
use crate::native::rng::EngineRng;

fn machine(num_rules: usize) -> NativeStateMachine {
    machine_concurrent(num_rules, 1)
}

fn machine_concurrent(num_rules: usize, concurrency: i64) -> NativeStateMachine {
    let names = (0..num_rules).map(|i| format!("rule_{i}")).collect();
    NativeStateMachine::new(
        vec!["g".to_string()],
        names,
        vec![0; num_rules],
        vec!["inv".to_string()],
        concurrency,
    )
}

fn grouped_machine(rule_groups: &[usize], num_groups: usize) -> NativeStateMachine {
    let names = (0..rule_groups.len())
        .map(|i| format!("rule_{i}"))
        .collect();
    let group_names = (0..num_groups).map(|g| format!("group_{g}")).collect();
    NativeStateMachine::new(
        group_names,
        names,
        rule_groups.to_vec(),
        vec!["inv".to_string()],
        1,
    )
}

fn replay(prefix: &[ChoiceValue], max_size: usize) -> NativeTestCase {
    NativeTestCase::for_choices_and_template(prefix, None, None, max_size, None)
}

fn simplest_after(prefix: &[ChoiceValue], max_size: usize) -> NativeTestCase {
    NativeTestCase::for_choices_and_template(
        prefix,
        None,
        Some(ChoiceTemplate::simplest(None)),
        max_size,
        None,
    )
}

fn int(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

/// A cap choice larger than every cap maximum, so the drawn cap truncates
/// to its maximum and never makes `next_group` / `next_rule` halt within a
/// test.
fn cap() -> ChoiceValue {
    int(1_000_000)
}

/// The node recording the rule index chosen by the enumeration fallback:
/// forced, and in the same `[0, n-1]` domain as the rejection-sampling tries.
fn assert_forced_index_node(ntc: &NativeTestCase, pos: usize, n: i64, index: i64) {
    let node = &ntc.nodes[pos];
    assert!(node.was_forced);
    assert_eq!(node.value, ChoiceValue::Integer(BigInt::from(index)));
    assert!(matches!(&*node.kind, ChoiceKind::Integer(k) if k.max_value == BigInt::from(n - 1)));
}

fn count_draws_with_max(ntc: &NativeTestCase, max_value: i64) -> usize {
    ntc.nodes
        .iter()
        .filter(
            |n| matches!(&*n.kind, ChoiceKind::Integer(k) if k.max_value == BigInt::from(max_value)),
        )
        .count()
}

#[test]
fn zero_p_disabled_enables_every_rule() {
    let mut ntc = replay(&[cap(), int(0), int(2)], 8);
    let mut sm = machine(3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap();
    assert_eq!(rule, Some(2));
    assert_eq!(ntc.nodes.len(), 4);
    assert!(ntc.nodes[3].was_forced);
    assert_eq!(ntc.nodes[3].value, ChoiceValue::Boolean(false));
    assert_eq!(ntc.spans.len(), 2);
    assert_eq!(
        ntc.spans[0usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_INTEGER as u64).to_string()
    );
    assert_eq!(
        ntc.spans[1usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG as u64).to_string()
    );
    assert!(!ntc.spans[1usize].discarded);
}

#[test]
fn round_cap_truncates_to_max_and_next_group_halts_after_that_many_rounds() {
    let mut ntc = simplest_after(&[cap()], 4096);
    let mut sm = machine(2);
    for _ in 0..MAX_SEQUENTIAL_ROUND_CAP {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    }
    let drawn = ntc.nodes.len();
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
    assert_eq!(ntc.nodes.len(), drawn);
}

#[test]
fn small_round_cap_halts_after_that_many_rounds() {
    let mut ntc = simplest_after(&[int(2)], 64);
    let mut sm = machine(2);
    for _ in 0..2 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert!(sm.next_rule(&mut ntc, 0).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn sequential_machine_hands_out_exactly_one_rule_per_round() {
    let mut ntc = simplest_after(&[cap()], 4096);
    let mut sm = machine(2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(sm.next_rule(&mut ntc, 0).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(sm.next_rule(&mut ntc, 0).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
}

#[test]
fn unbounded_families_draw_no_caps_and_next_group_never_halts() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    ntc.family().set_state_machine_steps_unbounded();
    let mut sm = machine(2);
    for _ in 0..2 * MAX_SEQUENTIAL_ROUND_CAP {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert!(sm.next_rule(&mut ntc, 0).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    }
    assert_eq!(count_draws_with_max(&ntc, i64::MAX), 0);
}

#[test]
fn p_disabled_is_drawn_on_first_next_rule_only() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let mut sm = machine(3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    sm.next_rule(&mut ntc, 0).unwrap();
    sm.next_group(&mut ntc).unwrap();
    sm.next_rule(&mut ntc, 0).unwrap();
    assert_eq!(count_draws_with_max(&ntc, 254), 1);
}

#[test]
fn last_undecided_rule_is_forced_enabled() {
    let prefix = [cap(), int(254), int(0), ChoiceValue::Boolean(true), int(1)];
    let mut ntc = replay(&prefix, 8);
    let mut sm = machine(2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 1);
    assert_eq!(ntc.nodes.len(), 6);
    assert!(ntc.nodes[5].was_forced);
    assert_eq!(ntc.nodes[5].value, ChoiceValue::Boolean(false));
}

#[test]
fn decided_flag_is_rewritten_as_forced_draw_on_later_queries() {
    let prefix = [cap(), int(254), int(0), ChoiceValue::Boolean(false), int(0)];
    let mut ntc = replay(&prefix, 8);
    let mut sm = machine(2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap().unwrap(), 0);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap().unwrap(), 0);
    assert_eq!(ntc.nodes.len(), 6);
    assert!(ntc.nodes[5].was_forced);
    assert_eq!(ntc.nodes[5].value, ChoiceValue::Boolean(false));
}

#[test]
fn known_disabled_rule_is_skipped_without_redrawing_its_flag() {
    let prefix = [
        cap(),
        int(254),
        int(1),
        ChoiceValue::Boolean(true),
        int(1),
        int(2),
        ChoiceValue::Boolean(false),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = machine(3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 7);
}

#[test]
fn fallback_early_exits_at_the_speculative_index() {
    let prefix = [
        cap(),
        int(254),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = machine(3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 10);
    assert_forced_index_node(&ntc, 9, 3, 2);
}

#[test]
fn fallback_draws_from_allowed_when_speculative_index_is_past_the_end() {
    let prefix = [
        cap(),
        int(254),
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
    let mut sm = machine(4);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 3);
    assert_eq!(ntc.nodes.len(), 12);
    assert!(ntc.nodes[9].was_forced);
    assert_forced_index_node(&ntc, 11, 4, 3);
}

#[test]
fn next_group_draws_and_returns_the_current_group_when_there_are_several() {
    let prefix = [cap(), int(1), int(254), int(0)];
    let mut ntc = replay(&prefix, 8);
    let mut sm = grouped_machine(&[0, 0, 1], 2);
    assert_eq!(sm.next_group(&mut ntc).unwrap(), Some(1));
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 5);
    assert!(ntc.nodes[4].was_forced);
    assert_eq!(ntc.nodes[4].value, ChoiceValue::Boolean(false));
}

#[test]
fn selection_stays_in_the_current_group() {
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        ntc.family().set_state_machine_steps_unbounded();
        let mut sm = grouped_machine(&[0, 1, 0, 1, 1], 2);
        for _ in 0..30 {
            let group = sm.next_group(&mut ntc).unwrap().unwrap();
            let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap() as usize;
            assert_eq!([0, 1, 0, 1, 1][rule], group);
        }
    }
}

#[test]
fn at_least_one_rule_per_group_is_forced_enabled() {
    let prefix = [
        cap(),
        int(0),
        int(254),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
    ];
    let mut ntc = replay(&prefix, 16);
    let mut sm = grouped_machine(&[0, 0, 1], 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    let rule = sm.next_rule(&mut ntc, 0).unwrap().unwrap();
    assert_eq!(rule, 1);
    assert_eq!(ntc.nodes.len(), 7);
    assert!(ntc.nodes[6].was_forced);
    assert_eq!(ntc.nodes[6].value, ChoiceValue::Boolean(false));
}

#[test]
fn concurrent_threads_draw_their_own_step_caps_and_flags() {
    let mut ntc = simplest_after(&[cap()], 4096);
    let mut sm = machine_concurrent(2, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert_eq!(sm.next_rule(&mut ntc, 1).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 1).unwrap(), None);
    assert_eq!(count_draws_with_max(&ntc, 254), 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    assert_eq!(sm.next_rule(&mut ntc, 1).unwrap(), Some(0));
}

#[test]
fn concurrent_per_round_step_cap_truncates_to_its_max() {
    let mut ntc = simplest_after(&[cap(), cap()], 4096);
    let mut sm = machine_concurrent(2, 2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    for _ in 0..MAX_ROUND_STEP_CAP {
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), Some(0));
    }
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
    assert_eq!(sm.next_rule(&mut ntc, 0).unwrap(), None);
}

#[test]
fn concurrent_round_cap_truncates_to_its_max() {
    let mut ntc = simplest_after(&[cap()], 4096);
    let mut sm = machine_concurrent(2, 3);
    for _ in 0..MAX_CONCURRENT_ROUND_CAP {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
    }
    assert!(sm.next_group(&mut ntc).unwrap().is_none());
}

#[test]
fn next_rule_before_next_group_is_an_invalid_argument() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let mut sm = machine(2);
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::InvalidArgument(_))
    ));
}

#[test]
fn out_of_range_thread_index_is_an_invalid_argument() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let mut sm = machine_concurrent(2, 2);
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

#[test]
fn overrun_while_drawing_the_round_cap_propagates() {
    let mut ntc = replay(&[], 0);
    let mut sm = machine(2);
    assert!(matches!(sm.next_group(&mut ntc), Err(EngineError::Overrun)));
}

#[test]
fn overrun_while_drawing_p_disabled_propagates() {
    let mut ntc = replay(&[cap()], 1);
    let mut sm = machine(2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_drawing_a_try_index_propagates() {
    let mut ntc = replay(&[cap(), int(0)], 2);
    let mut sm = machine(2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_recording_the_early_exit_index_propagates() {
    let prefix = [
        cap(),
        int(254),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 9);
    let mut sm = machine(3);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_recording_the_post_loop_index_propagates() {
    let prefix = [
        cap(),
        int(254),
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
    let mut sm = machine(4);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_inside_is_enabled_leaves_the_span_open_until_freeze() {
    let mut ntc = replay(&[cap(), int(254), int(0)], 3);
    let mut sm = machine(2);
    assert!(sm.next_group(&mut ntc).unwrap().is_some());
    assert!(matches!(
        sm.next_rule(&mut ntc, 0),
        Err(EngineError::Overrun)
    ));
    assert_eq!(ntc.status(), Some(Status::EarlyStop));
    ntc.freeze();
    assert_eq!(ntc.spans.len(), 2);
    assert_eq!(
        ntc.spans[1usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG as u64).to_string()
    );
    assert_eq!(ntc.spans[1usize].start, 3);
    assert_eq!(ntc.spans[1usize].end, 3);
}

#[test]
fn all_selected_rules_are_in_range() {
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        ntc.family().set_state_machine_steps_unbounded();
        let mut sm = machine(5);
        for _ in 0..30 {
            assert!(sm.next_group(&mut ntc).unwrap().is_some());
            assert!(sm.next_rule(&mut ntc, 0).unwrap().unwrap() < 5);
        }
    }
}

#[test]
fn simplest_template_always_selects_rule_zero() {
    let mut ntc = simplest_after(&[], 64);
    ntc.family().set_state_machine_steps_unbounded();
    let mut sm = machine(3);
    for _ in 0..5 {
        assert!(sm.next_group(&mut ntc).unwrap().is_some());
        assert_eq!(sm.next_rule(&mut ntc, 0).unwrap().unwrap(), 0);
    }
}

#[test]
#[should_panic(expected = "Stateful testing: there must be at least one rule")]
fn no_rules_is_error() {
    machine(0);
}

#[test]
#[should_panic(expected = "Stateful testing: there must be at least one concurrency group")]
fn no_groups_is_error() {
    NativeStateMachine::new(Vec::new(), vec!["rule".to_string()], vec![0], Vec::new(), 1);
}

#[test]
#[should_panic(expected = "Stateful testing: rule_groups must be parallel to rule_names")]
fn non_parallel_rule_groups_is_error() {
    NativeStateMachine::new(
        vec!["g".to_string()],
        vec!["rule".to_string()],
        vec![0, 0],
        Vec::new(),
        1,
    );
}

#[test]
#[should_panic(expected = "Stateful testing: rule group index out of range")]
fn out_of_range_rule_group_is_error() {
    NativeStateMachine::new(
        vec!["g".to_string()],
        vec!["rule".to_string()],
        vec![1],
        Vec::new(),
        1,
    );
}

#[test]
#[should_panic(expected = "Stateful testing: every concurrency group must have at least one rule")]
fn empty_group_is_error() {
    NativeStateMachine::new(
        vec!["g0".to_string(), "g1".to_string()],
        vec!["rule".to_string()],
        vec![0],
        Vec::new(),
        1,
    );
}

#[test]
#[should_panic(expected = "Stateful testing: concurrency must be at least 1")]
fn zero_concurrency_is_error() {
    NativeStateMachine::new(
        vec!["g".to_string()],
        vec!["rule".to_string()],
        vec![0],
        Vec::new(),
        0,
    );
}
