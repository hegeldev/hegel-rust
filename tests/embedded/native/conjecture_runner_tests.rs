use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::native::core::{BooleanChoice, ChoiceKind, ChoiceNode, ChoiceValue, Status};

fn make_rng() -> SmallRng {
    SmallRng::seed_from_u64(0)
}

fn default_settings() -> NativeRunnerSettings {
    NativeRunnerSettings::new()
        .max_examples(10)
        .suppress_health_check(vec![
            HealthCheckLabel::FilterTooMuch,
            HealthCheckLabel::TooSlow,
            HealthCheckLabel::LargeBaseExample,
            HealthCheckLabel::DataTooLarge,
        ])
}

// ── NativeRunnerSettings builder methods ──────────────────────────────────

#[test]
fn settings_report_multiple_bugs_builder() {
    let s = NativeRunnerSettings::new().report_multiple_bugs(false);
    assert!(!s.report_multiple_bugs);
}

#[test]
fn settings_buffer_size_limit_builder() {
    let s = NativeRunnerSettings::new().buffer_size_limit(1024);
    assert_eq!(s.buffer_size_limit, Some(1024));
}

#[test]
fn settings_cache_size_builder() {
    let s = NativeRunnerSettings::new().cache_size(500);
    assert_eq!(s.cache_size, Some(500));
}

#[test]
fn settings_default() {
    let s = NativeRunnerSettings::default();
    assert_eq!(s.max_examples, 100);
    assert!(s.report_multiple_bugs);
    assert!(s.buffer_size_limit.is_none());
    assert!(s.cache_size.is_none());
}

// ── InterestingOrigin::from_panic_payload — type-id branch ────────────────

#[test]
fn from_panic_payload_type_id_branch() {
    // Run a test that panics with a non-str non-String payload (u64).
    // The runner should record it as Interesting with a type-id label.
    let settings = default_settings();
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let _ = data.draw_integer(0, 100);
            std::panic::panic_any(42u64);
        },
        settings,
        make_rng(),
    );
    runner.run();
    assert!(!runner.interesting_examples.is_empty());
    // Verify that the origin has a type-id label.
    let (origin, _) = runner.interesting_examples.iter().next().unwrap();
    let label = origin.panic_label.as_deref().unwrap_or("");
    assert!(label.starts_with("type-id:"), "label was: {label}");
}

// ── dominance() — Equal keys ─────────────────────────────────────────────

#[test]
fn dominance_equal_keys_returns_equal() {
    let result = ConjectureRunResult {
        status: Status::Valid,
        nodes: vec![],
        choices: vec![],
        target_observations: Default::default(),
        origin: None,
        tags: Default::default(),
    };
    let d = dominance(&result, &result.clone());
    assert_eq!(d, DominanceRelation::Equal);
}

// ── dominance() — right simpler, no dominance (other => other branch) ────

#[test]
fn dominance_right_simpler_no_dominance() {
    // right has a shorter sort_key (simpler). The recursion is:
    //   dominance(left={longer}, right={shorter})
    //   → right_key < left_key, recurse: dominance(right={shorter}, left={longer})
    //   → left={shorter} has empty tags; right={longer} has tag {42}
    //   → right.tags.is_subset(left.tags) = {42}.is_subset({}) = false → NoDominance
    //   → original: match NoDominance => NoDominance (the `other => other` branch)
    let mut longer_tags = std::collections::HashSet::new();
    longer_tags.insert(42u64);
    let longer = ConjectureRunResult {
        status: Status::Valid,
        nodes: vec![ChoiceNode {
            kind: ChoiceKind::Boolean(BooleanChoice),
            value: ChoiceValue::Boolean(true),
            was_forced: false,
        }],
        choices: vec![ChoiceValue::Boolean(true)],
        target_observations: Default::default(),
        origin: None,
        tags: longer_tags,
    };
    let shorter = ConjectureRunResult {
        status: Status::Valid,
        nodes: vec![],
        choices: vec![],
        target_observations: Default::default(),
        origin: None,
        tags: Default::default(),
    };
    // Pass longer as left, shorter as right. The right_key < left_key branch fires.
    // The recursive call returns NoDominance. After the swap: NoDominance.
    let d = dominance(&longer, &shorter);
    assert_eq!(d, DominanceRelation::NoDominance);
}

// ── ParetoFront::try_add with RightDominates ──────────────────────────────

#[test]
fn pareto_front_right_dominates_evicts_worse_entry() {
    let mut front = ParetoFront::new(make_rng());
    // Add a "worse" entry (longer node sequence).
    let worse = ConjectureRunResult {
        status: Status::Valid,
        nodes: vec![
            ChoiceNode {
                kind: ChoiceKind::Boolean(crate::native::core::BooleanChoice),
                value: ChoiceValue::Boolean(true),
                was_forced: false,
            },
            ChoiceNode {
                kind: ChoiceKind::Boolean(crate::native::core::BooleanChoice),
                value: ChoiceValue::Boolean(true),
                was_forced: false,
            },
        ],
        choices: vec![ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)],
        target_observations: Default::default(),
        origin: None,
        tags: Default::default(),
    };
    front.add(worse.clone());
    assert_eq!(front.len(), 1);

    // Add a "better" entry (empty → simpler, covers the same tags).
    let better = ConjectureRunResult {
        status: Status::Valid,
        nodes: vec![],
        choices: vec![],
        target_observations: Default::default(),
        origin: None,
        tags: Default::default(),
    };
    let (in_front, evicted) = front.add(better);
    assert!(in_front);
    assert!(!evicted.is_empty());
}

// ── ParetoFront::try_add — Equal case ────────────────────────────────────

#[test]
fn pareto_front_adding_equal_entry_is_idempotent() {
    let mut front = ParetoFront::new(make_rng());
    let entry = ConjectureRunResult {
        status: Status::Valid,
        nodes: vec![],
        choices: vec![],
        target_observations: Default::default(),
        origin: None,
        tags: Default::default(),
    };
    front.add(entry.clone());
    let (in_front, evicted) = front.add(entry);
    assert!(in_front);
    assert!(evicted.is_empty());
    assert_eq!(front.len(), 1);
}

// ── ParetoFront::iter() ──────────────────────────────────────────────────

#[test]
fn pareto_front_iter_nonempty() {
    let mut front = ParetoFront::new(make_rng());
    let entry = ConjectureRunResult {
        status: Status::Valid,
        nodes: vec![],
        choices: vec![],
        target_observations: Default::default(),
        origin: None,
        tags: Default::default(),
    };
    front.add(entry);
    let v: Vec<_> = front.iter().collect();
    assert_eq!(v.len(), 1);
}

// ── ParetoFront::is_empty() ──────────────────────────────────────────────

#[test]
fn pareto_front_is_empty_on_new() {
    let front = ParetoFront::new(make_rng());
    assert!(front.is_empty());
}

// ── NativeConjectureData::draw_bytes_forced — buffer size limit ───────────

#[test]
fn draw_bytes_forced_exceeds_buffer_triggers_stop_test() {
    use std::panic::AssertUnwindSafe;
    use std::panic::catch_unwind;
    let mut data = NativeConjectureData::for_choices(&[]);
    // Override the buffer_size_limit to something tiny.
    // We can't set it directly (private), so use the runner path.
    // Instead: call draw_bytes_forced in a test that has a very small limit.
    // We use `for_choices` which defaults to CONJECTURE_BUFFER_SIZE.
    // Trigger the limit by calling with a large forced vec.
    // Since bytes_drawn starts at 0 and buffer_size_limit is 8192,
    // we need forced.len() > 8192 to trigger. Do that:
    let forced = vec![0u8; 8193];
    let result = catch_unwind(AssertUnwindSafe(|| {
        data.draw_bytes_forced(0, 10000, forced);
    }));
    assert!(result.is_err());
}

// ── NativeConjectureData::stop_span_with_discard(true) ───────────────────

#[test]
fn stop_span_with_discard_sets_has_discards() {
    let mut data = NativeConjectureData::for_choices(&[]);
    data.start_span(1);
    data.stop_span_with_discard(true);
    assert!(data.ntc.has_discards);
}

// ── NativeConjectureData::nodes() and choices() ──────────────────────────

#[test]
fn nodes_and_choices_reflect_draws() {
    use crate::native::core::ChoiceValue;
    let choices = vec![ChoiceValue::Boolean(true)];
    let mut data = NativeConjectureData::for_choices(&choices);
    let v = data.draw_boolean(0.5);
    assert!(v);
    assert_eq!(data.nodes().len(), 1);
    let ch = data.choices();
    assert_eq!(ch.len(), 1);
    assert_eq!(ch[0], ChoiceValue::Boolean(true));
}

// ── NativeConjectureData::status() ───────────────────────────────────────

#[test]
fn data_status_returns_valid_initially() {
    let data = NativeConjectureData::for_choices(&[]);
    assert_eq!(data.status(), Status::Valid);
}

#[test]
fn data_status_returns_invalid_after_mark_invalid() {
    use std::panic::AssertUnwindSafe;
    use std::panic::catch_unwind;
    let mut data = NativeConjectureData::for_choices(&[]);
    let _ = catch_unwind(AssertUnwindSafe(|| {
        data.mark_invalid(None);
    }));
    assert_eq!(data.status(), Status::Invalid);
}

#[test]
fn data_status_returns_interesting_after_mark_interesting() {
    use std::panic::AssertUnwindSafe;
    use std::panic::catch_unwind;
    let mut data = NativeConjectureData::for_choices(&[]);
    let _ = catch_unwind(AssertUnwindSafe(|| {
        data.mark_interesting(interesting_origin(None));
    }));
    assert_eq!(data.status(), Status::Interesting);
}

// ── NativeDataTreeView::simulate_test_function returning false ────────────

#[test]
fn simulate_test_function_returns_false_for_unknown_path() {
    let settings = default_settings();
    let runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            let v = data.draw_boolean(0.5);
            if v {
                data.mark_interesting(interesting_origin(None));
            }
        },
        settings,
        make_rng(),
    );
    // Without any run, the tree is empty — simulate on any choices returns false.
    let choices = vec![ChoiceValue::Boolean(true)];
    assert!(!runner.tree().simulate_test_function(&choices));
}

// ── run_shrinker_user_fn with arbitrary panic ─────────────────────────────

#[test]
fn run_shrinker_user_fn_arbitrary_panic_returns_interesting() {
    let ntc = crate::native::core::NativeTestCase::for_choices(&[], None, None);
    let (interesting, _, _, _) = run_shrinker_user_fn(
        &mut |_data: &mut NativeConjectureData| {
            panic!("user error");
        },
        ntc,
    );
    assert!(interesting);
}

// ── NativeConjectureRunner::new_shrinker todo ─────────────────────────────

#[test]
#[should_panic(expected = "NativeConjectureRunner::new_shrinker")]
fn new_shrinker_panics_with_todo() {
    let settings = default_settings();
    let mut runner =
        NativeConjectureRunner::new(|_data: &mut NativeConjectureData| {}, settings, make_rng());
    let data = NativeConjectureData::for_choices(&[]);
    runner.new_shrinker(data, |_d: &NativeConjectureData| true);
}

// ── ChoiceValueKey::String ────────────────────────────────────────────────

#[test]
fn choice_value_key_string_variant() {
    let v = ChoiceValue::String(vec![65, 66, 67]);
    let key = ChoiceValueKey::from(&v);
    assert!(matches!(key, ChoiceValueKey::String(_)));
}

// ── No-read no-shrink path: test marks interesting without any draws ───────

#[test]
fn no_read_no_shrink_initial_is_empty_skips_shrink() {
    // A test that marks interesting without any draws produces an empty
    // initial node sequence. shrink_interesting_examples skips it.
    let settings = NativeRunnerSettings::new()
        .max_examples(10)
        .suppress_health_check(vec![
            HealthCheckLabel::FilterTooMuch,
            HealthCheckLabel::TooSlow,
            HealthCheckLabel::LargeBaseExample,
            HealthCheckLabel::DataTooLarge,
        ]);
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.mark_interesting(interesting_origin(None));
        },
        settings,
        make_rng(),
    );
    runner.run();
    assert!(!runner.interesting_examples.is_empty());
}

// ── fails_health_check panics when run() returns normally ─────────────────

#[test]
#[should_panic(expected = "expected a FailedHealthCheck panic")]
fn fails_health_check_panics_when_no_panic() {
    // If the runner never raises a health check panic, fails_health_check
    // should itself panic with the "expected a FailedHealthCheck" message.
    fails_health_check(HealthCheckLabel::FilterTooMuch, || {
        let settings = NativeRunnerSettings::new()
            .max_examples(1)
            .suppress_health_check(vec![
                HealthCheckLabel::FilterTooMuch,
                HealthCheckLabel::TooSlow,
                HealthCheckLabel::LargeBaseExample,
                HealthCheckLabel::DataTooLarge,
            ]);
        NativeConjectureRunner::new(|_data: &mut NativeConjectureData| {}, settings, make_rng())
    });
}
