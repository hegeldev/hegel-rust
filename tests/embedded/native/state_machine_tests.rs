// Embedded tests for swarm rule selection (NativeStateMachine / FeatureFlags).
//
// Tests drive `next_rule` with explicit choice prefixes
// (`for_choices_and_template`) so each path through the rejection-sampling
// loop and the enumeration fallback is reached deterministically. Prefix
// positions consumed by *forced* draws are never read (forced draws skip the
// prefix but still occupy a node position), so prefixes only spell out the
// unforced draws, with padding where a forced node sits between them.

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

// ── shrink open ──────────────────────────────────────────────────────────

#[test]
fn zero_p_disabled_enables_every_rule() {
    // p_disabled draw of 0 is the shrunk value: every is_enabled check is
    // forced enabled (weighted with p = 0 short-circuits), so swarm
    // restrictions vanish from minimal test cases.
    let mut ntc = replay(&[int(0), int(2)], 8);
    let rule = machine(3).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 3);
    // The enabled-check is recorded as a forced boolean.
    assert!(ntc.nodes[2].was_forced);
    assert_eq!(ntc.nodes[2].value, ChoiceValue::Boolean(false));
    // The check is wrapped in a FEATURE_FLAG span.
    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(ntc.spans[0usize].label, labels::FEATURE_FLAG.to_string());
    assert!(!ntc.spans[0usize].discarded);
}

// ── flags are created lazily, once ───────────────────────────────────────

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

// ── at_least_one_of ──────────────────────────────────────────────────────

#[test]
fn last_undecided_rule_is_forced_enabled() {
    // n=2, p_disabled at maximum: rule 0 is disabled by the prefix, which
    // leaves rule 1 as the only at_least_one_of candidate — its check is
    // forced enabled rather than drawn.
    let prefix = [
        int(254),                   // p_disabled
        int(0),                     // try 1: rule 0
        ChoiceValue::Boolean(true), // rule 0 disabled
        int(1),                     // try 2: rule 1
                                    // is_enabled(1) is forced — no prefix entry
    ];
    let mut ntc = replay(&prefix, 8);
    let rule = machine(2).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 1);
    assert_eq!(ntc.nodes.len(), 5);
    assert!(ntc.nodes[4].was_forced);
    assert_eq!(ntc.nodes[4].value, ChoiceValue::Boolean(false));
}

// ── decided flags are re-recorded as forced draws ────────────────────────

#[test]
fn decided_flag_is_rewritten_as_forced_draw_on_later_queries() {
    let prefix = [
        int(254),                    // p_disabled
        int(0),                      // first next_rule, try 1: rule 0
        ChoiceValue::Boolean(false), // rule 0 enabled
        int(0),                      // second next_rule, try 1: rule 0 again
                                     // re-check of rule 0 is forced
    ];
    let mut ntc = replay(&prefix, 8);
    let mut sm = machine(2);
    assert_eq!(sm.next_rule(&mut ntc).unwrap(), 0);
    assert_eq!(sm.next_rule(&mut ntc).unwrap(), 0);
    assert_eq!(ntc.nodes.len(), 5);
    // The second query re-writes the earlier decision to the choice
    // sequence as a forced draw, so deleting the original deciding draw
    // during shrinking just moves the decision point.
    assert!(ntc.nodes[4].was_forced);
    assert_eq!(ntc.nodes[4].value, ChoiceValue::Boolean(false));
}

// ── rejection sampling ────────────────────────────────────────────────────

#[test]
fn known_disabled_rule_is_skipped_without_redrawing_its_flag() {
    let prefix = [
        int(254),                    // p_disabled
        int(1),                      // try 1: rule 1
        ChoiceValue::Boolean(true),  // rule 1 disabled
        int(1),                      // try 2: rule 1 again — known bad, no flag draw
        int(2),                      // try 3: rule 2
        ChoiceValue::Boolean(false), // rule 2 enabled
    ];
    let mut ntc = replay(&prefix, 16);
    let rule = machine(3).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 6);
}

// ── enumeration fallback ─────────────────────────────────────────────────

#[test]
fn fallback_early_exits_at_the_speculative_index() {
    // All three tries fail (two distinct disabled rules, one known-bad
    // repeat), so next_rule falls back to enumeration. The speculative
    // index 0 is reached as soon as rule 2 — the forced-enabled last
    // candidate — joins the allowed list.
    let prefix = [
        int(254),                   // p_disabled
        int(0),                     // try 1: rule 0
        ChoiceValue::Boolean(true), // rule 0 disabled
        int(1),                     // try 2: rule 1
        ChoiceValue::Boolean(true), // rule 1 disabled
        int(0),                     // try 3: rule 0 — known bad
        int(0),                     // speculative index in [0, 0]
                                    // is_enabled(2) forced, chosen index forced
    ];
    let mut ntc = replay(&prefix, 16);
    let rule = machine(3).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 2);
    assert_eq!(ntc.nodes.len(), 9);
    assert_forced_index_node(&ntc, 8, 3, 2);
}

#[test]
fn fallback_draws_from_allowed_when_speculative_index_is_past_the_end() {
    // n=4 with rules 0 and 1 disabled in the tries. The fallback's
    // speculative index (1) is never reached: rule 2 is disabled during
    // enumeration, leaving only rule 3 in the allowed list. The chosen
    // rule is then drawn uniformly from the allowed list.
    let prefix = [
        int(254),                   // p_disabled
        int(0),                     // try 1: rule 0
        ChoiceValue::Boolean(true), // rule 0 disabled
        int(1),                     // try 2: rule 1
        ChoiceValue::Boolean(true), // rule 1 disabled
        int(0),                     // try 3: rule 0 — known bad
        int(1),                     // speculative index in [0, 1]
        ChoiceValue::Boolean(true), // rule 2 disabled during enumeration
        int(0),                     // padding: position 8 is the forced is_enabled(3)
        int(0),                     // index into the allowed list [3]
                                    // chosen index forced
    ];
    let mut ntc = replay(&prefix, 16);
    let rule = machine(4).next_rule(&mut ntc).unwrap();
    assert_eq!(rule, 3);
    assert_eq!(ntc.nodes.len(), 11);
    // is_enabled(3) was forced: last at_least_one_of candidate.
    assert!(ntc.nodes[8].was_forced);
    assert_forced_index_node(&ntc, 10, 4, 3);
}

// ── overrun behaviour ────────────────────────────────────────────────────

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
    // Same trace as fallback_early_exits_at_the_speculative_index, but the
    // choice budget runs out exactly at the final forced index draw.
    let prefix = [
        int(254),                   // p_disabled
        int(0),                     // try 1: rule 0
        ChoiceValue::Boolean(true), // rule 0 disabled
        int(1),                     // try 2: rule 1
        ChoiceValue::Boolean(true), // rule 1 disabled
        int(0),                     // try 3: rule 0 — known bad
        int(0),                     // speculative index in [0, 0]
    ];
    let mut ntc = replay(&prefix, 8);
    assert!(matches!(
        machine(3).next_rule(&mut ntc),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn overrun_while_recording_the_post_loop_index_propagates() {
    // Same trace as fallback_draws_from_allowed_when_speculative_index_is
    // _past_the_end, but the choice budget runs out exactly at the final
    // forced index draw.
    let prefix = [
        int(254),                   // p_disabled
        int(0),                     // try 1: rule 0
        ChoiceValue::Boolean(true), // rule 0 disabled
        int(1),                     // try 2: rule 1
        ChoiceValue::Boolean(true), // rule 1 disabled
        int(0),                     // try 3: rule 0 — known bad
        int(1),                     // speculative index in [0, 1]
        ChoiceValue::Boolean(true), // rule 2 disabled during enumeration
        int(0),                     // padding: position 8 is the forced is_enabled(3)
        int(0),                     // index into the allowed list [3]
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
    assert_eq!(ntc.status, Some(Status::EarlyStop));
    // The FEATURE_FLAG span was opened before the failing draw; freeze
    // closes intervals left open by the overrun.
    ntc.freeze();
    assert_eq!(ntc.spans.len(), 1);
    assert_eq!(ntc.spans[0usize].label, labels::FEATURE_FLAG.to_string());
    assert_eq!(ntc.spans[0usize].start, 2);
    assert_eq!(ntc.spans[0usize].end, 2);
}

// ── selection statistics ─────────────────────────────────────────────────

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
    // Under the all-simplest template (the shrinker's "left leaf" probe)
    // p_disabled is 0, every rule is enabled, and the first try's uniform
    // index draw resolves to its simplest value 0.
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

// invalid argument
#[test]
#[should_panic(expected = "Stateful testing: there must be at least one rule")]
fn no_rules_is_error() {
    machine(0);
}
