use super::*;
use crate::native::core::choices::ChoiceTemplate;
use crate::native::core::{ChoiceKind, ChoiceValue, Status};
use crate::native::rng::EngineRng;

fn machine(num_rules: usize) -> NativeStateMachine {
    let names = (0..num_rules).map(|i| format!("rule_{i}")).collect();
    NativeStateMachine::new(names, vec!["inv".to_string()])
}

fn replay(prefix: &[ChoiceValue], max_size: usize) -> NativeTestCase {
    NativeTestCase::for_choices_and_template(prefix, None, None, max_size, None)
}

fn int(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

/// The node recording the rule index chosen by the enumeration fallback:
/// forced, and in the same `[0, n-1]` domain as the rejection-sampling tries.
fn assert_forced_index_node(ntc: &NativeTestCase, pos: usize, n: i64, index: i64) {
    let node = &ntc.nodes[pos];
    assert!(node.was_forced);
    assert_eq!(node.value, ChoiceValue::Integer(BigInt::from(index)));
    assert!(matches!(&*node.kind, ChoiceKind::Integer(k) if k.max_value == BigInt::from(n - 1)));
}

#[test]
fn zero_p_disabled_enables_every_rule() {
    let mut ntc = replay(&[int(0), int(2)], 8);
    let rule = machine(3).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 3);
    assert!(ntc.nodes[2].was_forced);
    assert_eq!(ntc.nodes[2].value, ChoiceValue::Boolean(false));
    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(
        ntc.spans[0usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG as u64).to_string()
    );
    assert!(!ntc.spans[0usize].discarded);
}

#[test]
fn p_disabled_is_drawn_on_first_next_rule_only() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let mut sm = machine(3);
    sm.next_rule(&mut ntc).unwrap();
    sm.next_rule(&mut ntc).unwrap();
    let p_disabled_draws = ntc
        .nodes
        .iter()
        .filter(|n| matches!(&*n.kind, ChoiceKind::Integer(k) if k.max_value == BigInt::from(254)))
        .count();
    assert_eq!(p_disabled_draws, 1);
}

#[test]
fn last_undecided_rule_is_forced_enabled() {
    let prefix = [int(254), int(0), ChoiceValue::Boolean(true), int(1)];
    let mut ntc = replay(&prefix, 8);
    let rule = machine(2).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 1);
    assert_eq!(ntc.nodes.len(), 5);
    assert!(ntc.nodes[4].was_forced);
    assert_eq!(ntc.nodes[4].value, ChoiceValue::Boolean(false));
}

#[test]
fn decided_flag_is_rewritten_as_forced_draw_on_later_queries() {
    let prefix = [int(254), int(0), ChoiceValue::Boolean(false), int(0)];
    let mut ntc = replay(&prefix, 8);
    let mut sm = machine(2);
    assert_eq!(sm.next_rule(&mut ntc).unwrap(), 0);
    assert_eq!(sm.next_rule(&mut ntc).unwrap(), 0);
    assert_eq!(ntc.nodes.len(), 5);
    assert!(ntc.nodes[4].was_forced);
    assert_eq!(ntc.nodes[4].value, ChoiceValue::Boolean(false));
}

#[test]
fn known_disabled_rule_is_skipped_without_redrawing_its_flag() {
    let prefix = [
        int(254),
        int(1),
        ChoiceValue::Boolean(true),
        int(1),
        int(2),
        ChoiceValue::Boolean(false),
    ];
    let mut ntc = replay(&prefix, 16);
    let rule = machine(3).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 6);
}

#[test]
fn fallback_early_exits_at_the_speculative_index() {
    let prefix = [
        int(254),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 16);
    let rule = machine(3).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 9);
    assert_forced_index_node(&ntc, 8, 3, 2);
}

#[test]
fn fallback_draws_from_allowed_when_speculative_index_is_past_the_end() {
    let prefix = [
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
    let rule = machine(4).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 3);
    assert_eq!(ntc.nodes.len(), 11);
    assert!(ntc.nodes[8].was_forced);
    assert_forced_index_node(&ntc, 10, 4, 3);
}

#[test]
fn overrun_while_drawing_p_disabled_propagates() {
    let mut ntc = replay(&[], 0);
    let mut sm = machine(2);
    assert!(matches!(sm.next_rule(&mut ntc), Err(EngineError::Overrun)));
}

#[test]
fn overrun_while_drawing_a_try_index_propagates() {
    let mut ntc = replay(&[int(0)], 1);
    let mut sm = machine(2);
    assert!(matches!(sm.next_rule(&mut ntc), Err(EngineError::Overrun)));
}

#[test]
fn overrun_while_recording_the_early_exit_index_propagates() {
    let prefix = [
        int(254),
        int(0),
        ChoiceValue::Boolean(true),
        int(1),
        ChoiceValue::Boolean(true),
        int(0),
        int(0),
    ];
    let mut ntc = replay(&prefix, 8);
    assert!(matches!(
        machine(3).next_rule(&mut ntc),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_recording_the_post_loop_index_propagates() {
    let prefix = [
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
    let mut ntc = replay(&prefix, 10);
    assert!(matches!(
        machine(4).next_rule(&mut ntc),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_inside_is_enabled_leaves_the_span_open_until_freeze() {
    let mut ntc = replay(&[int(254), int(0)], 2);
    let mut sm = machine(2);
    assert!(matches!(sm.next_rule(&mut ntc), Err(EngineError::Overrun)));
    assert_eq!(ntc.status(), Some(Status::EarlyStop));
    ntc.freeze();
    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(
        ntc.spans[0usize].label,
        (crate::hegel_label_t::HEGEL_LABEL_FEATURE_FLAG as u64).to_string()
    );
    assert_eq!(ntc.spans[0usize].start, 2);
    assert_eq!(ntc.spans[0usize].end, 2);
}

#[test]
fn all_selected_rules_are_in_range() {
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut sm = machine(5);
        for _ in 0..30 {
            assert!(sm.next_rule(&mut ntc).unwrap() < 5);
        }
    }
}

#[test]
fn simplest_template_always_selects_rule_zero() {
    let mut ntc = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(None)),
        64,
        None,
    );
    let mut sm = machine(3);
    for _ in 0..5 {
        assert_eq!(sm.next_rule(&mut ntc).unwrap(), 0);
    }
}

#[test]
#[should_panic(expected = "Stateful testing: there must be at least one rule")]
fn no_rules_is_error() {
    machine(0);
}
