use super::*;
use crate::native::core::GenerationParameters;
use crate::native::core::choices::BooleanChoice;
use crate::native::rng::EngineRng;

#[test]
fn spans_get_mut_returns_mutable_reference() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "test".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    let span = spans.get_mut(0).unwrap();
    span.discarded = true;
    assert!(spans[0usize].discarded);
}

#[test]
fn spans_get_mut_returns_none_out_of_bounds() {
    let mut spans = Spans::new();
    assert!(spans.get_mut(0).is_none());
}

#[test]
fn spans_trivial_handles_simplest_forced_and_oob() {
    use crate::native::core::choices::ChoiceNode;
    let simplest = ChoiceNode::boolean(BooleanChoice { p: 0.5 }, false, false);
    let interesting = ChoiceNode::boolean(BooleanChoice { p: 0.5 }, true, false);
    let forced_interesting = ChoiceNode::boolean(BooleanChoice { p: 0.5 }, true, true);

    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 2,
        label: "outer".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });

    let nodes = vec![simplest.clone(), simplest.clone()];
    assert!(spans.trivial(0, &nodes).unwrap());

    let nodes = vec![simplest.clone(), interesting.clone()];
    assert!(!spans.trivial(0, &nodes).unwrap());

    let nodes = vec![simplest, forced_interesting];
    assert!(spans.trivial(0, &nodes).unwrap());

    let other = Spans::new();
    let empty: Vec<ChoiceNode> = Vec::new();
    assert!(!other.trivial(7, &empty).unwrap());
}

#[test]
fn spans_into_vec_consumes_and_returns_inner() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "one".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    let v = spans.into_vec();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].label, "one");
}

#[test]
fn spans_from_vec() {
    let v = vec![Span {
        start: 0,
        end: 3,
        label: "x".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    }];
    let spans = Spans::from(v);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0usize].label, "x");
}

#[test]
fn spans_deref_to_slice() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "deref".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    let slice: &[Span] = &spans;
    assert_eq!(slice.len(), 1);
    assert_eq!(slice[0].label, "deref");
}

#[test]
fn spans_into_iterator() {
    let mut spans = Spans::new();
    for i in 0..3 {
        spans.push(Span {
            start: i,
            end: i + 1,
            label: i.to_string(),
            depth: 0,
            parent: None,
            discarded: false,
        });
    }
    let labels: Vec<&str> = (&spans).into_iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["0", "1", "2"]);
}

#[test]
fn draw_integer_forced_records_a_forced_node_without_consuming_the_prefix() {
    let prefix = vec![ChoiceValue::Integer(BigInt::from(9))];
    let mut tc = NativeTestCase::for_choices_and_template(
        &prefix,
        None,
        Some(ChoiceTemplate::simplest(None).unwrap()),
        4,
        None,
    );
    tc.draw_integer_forced(BigInt::from(0), BigInt::from(10), BigInt::from(7))
        .ok()
        .unwrap();
    assert_eq!(tc.nodes.len(), 1);
    assert!(tc.nodes[0].was_forced);
    assert_eq!(tc.nodes[0].value(), ChoiceValue::Integer(BigInt::from(7)));
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
}

#[test]
fn draw_integer_forced_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct ForcedIntObserver {
        captured: Arc<Mutex<Option<(BigInt, bool)>>>,
    }
    impl DataObserver for ForcedIntObserver {
        fn draw_integer(&mut self, value: &BigInt, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.clone(), was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let obs = Box::new(ForcedIntObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices_and_template(&[], None, None, 4, Some(obs));
    tc.draw_integer_forced(0i64, 5i64, 3i64).ok().unwrap();
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((BigInt::from(3), true)));
}

#[test]
fn draw_integer_forced_errors_on_an_exhausted_test_case() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    assert!(matches!(
        tc.draw_integer_forced(0i64, 5i64, 3i64),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn draw_integer_forced_rejects_out_of_range_values() {
    let mut tc = NativeTestCase::for_choices_and_template(&[], None, None, 4, None);
    let msg = tc
        .draw_integer_forced(0i64, 5i64, 6i64)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("outside"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn spans_get_returns_span_by_index() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "first".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    spans.push(Span {
        start: 1,
        end: 2,
        label: "second".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert_eq!(spans.get(0).unwrap().label, "first");
    assert_eq!(spans.get(1).unwrap().label, "second");
    assert!(spans.get(2).is_none());
}

#[test]
fn spans_as_slice_returns_slice() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "a".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    let sl = spans.as_slice();
    assert_eq!(sl.len(), 1);
    assert_eq!(sl[0].label, "a");
}

struct NoopObserver;
impl DataObserver for NoopObserver {}

#[test]
fn stop_span_on_empty_stack_is_a_no_op() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    tc.stop_span(false);
    assert!(tc.spans.is_empty());
}

#[test]
fn data_observer_draw_boolean_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_boolean(true, false);
}

#[test]
fn data_observer_draw_integer_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_integer(&BigInt::from(42), false);
}

#[test]
fn data_observer_draw_float_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_float(1.5, false);
}

#[test]
fn data_observer_conclude_test_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.conclude_test(Status::Valid, None);
}

#[test]
fn weighted_with_p_zero_returns_false_without_consulting_rng() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let v = tc.weighted(0.0, None).ok().unwrap();
    assert!(!v);
    assert!(tc.nodes.last().unwrap().was_forced);
}

#[test]
fn weighted_with_p_one_returns_true_without_consulting_rng() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let v = tc.weighted(1.0, None).ok().unwrap();
    assert!(v);
    assert!(tc.nodes.last().unwrap().was_forced);
}

#[test]
fn weighted_with_explicit_forced_records_forced_node() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let v = tc.weighted(0.5, Some(true)).ok().unwrap();
    assert!(v);
    assert!(tc.nodes.last().unwrap().was_forced);
    let v = tc.weighted(0.5, Some(false)).ok().unwrap();
    assert!(!v);
    assert!(tc.nodes.last().unwrap().was_forced);
}

#[test]
fn freeze_is_a_no_op_on_already_frozen_test_case() {
    let mut tc = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None, None);
    tc.start_span(7);
    tc.stop_span(false);
    tc.freeze();
    let spans_after_first = tc.spans.clone().into_vec();
    tc.freeze();
    assert_eq!(tc.spans.clone().into_vec(), spans_after_first);
}

#[test]
fn weighted_notifies_observer_on_boolean_draw() {
    use std::sync::{Arc, Mutex};
    struct CaptureBoolObserver {
        captured: Arc<Mutex<Option<(bool, bool)>>>,
    }
    impl DataObserver for CaptureBoolObserver {
        fn draw_boolean(&mut self, value: bool, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value, was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let obs = Box::new(CaptureBoolObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None, Some(obs));
    let v = tc.weighted(0.5, None).ok().unwrap();
    assert!(v);
    let recorded = captured.lock().unwrap().expect("observer wasn't called");
    assert_eq!(recorded, (true, false));
}

#[test]
fn freeze_notifies_observer_on_conclude_test() {
    use std::sync::{Arc, Mutex};
    struct FreezeObserver {
        captured: Arc<Mutex<Option<Status>>>,
    }
    impl DataObserver for FreezeObserver {
        fn conclude_test(&mut self, status: Status, _origin: Option<InterestingOrigin>) {
            *self.captured.lock().unwrap() = Some(status);
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let obs = Box::new(FreezeObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&[], None, Some(obs));
    tc.freeze();
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some(Status::Valid));
}

#[test]
fn draw_integer_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct IntObserver {
        captured: Arc<Mutex<Option<(BigInt, bool)>>>,
    }
    impl DataObserver for IntObserver {
        fn draw_integer(&mut self, value: &BigInt, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.clone(), was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::Integer(BigInt::from(99))];
    let obs = Box::new(IntObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_integer::<i128>(0, 100).ok().unwrap();
    assert_eq!(v, 99);
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((BigInt::from(99), false)));
}

#[test]
fn draw_float_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct FloatObserver {
        captured: Arc<Mutex<Option<(u64, bool)>>>,
    }
    impl DataObserver for FloatObserver {
        fn draw_float(&mut self, value: f64, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.to_bits(), was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::Float(2.5)];
    let obs = Box::new(FloatObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_float(0.0, 10.0, false, false, 5e-324).ok().unwrap();
    assert_eq!(v, 2.5);
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((2.5_f64.to_bits(), false)));
}

#[test]
fn data_observer_draw_bytes_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_bytes(&[1, 2, 3], false);
}

#[test]
fn draw_bytes_notifies_observer() {
    use std::sync::{Arc, Mutex};
    type Captured = Arc<Mutex<Option<(Vec<u8>, bool)>>>;
    struct BytesObserver {
        captured: Captured,
    }
    impl DataObserver for BytesObserver {
        fn draw_bytes(&mut self, value: &[u8], was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.to_vec(), was_forced));
        }
    }
    let captured: Captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::Bytes(vec![1, 2, 3])];
    let obs = Box::new(BytesObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_bytes(0, 10).ok().unwrap();
    assert_eq!(v, vec![1, 2, 3]);
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((vec![1u8, 2, 3], false)));
}

#[test]
fn data_observer_draw_string_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_string("hello", false);
}

#[test]
fn draw_string_notifies_observer() {
    use std::sync::{Arc, Mutex};
    type Captured = Arc<Mutex<Option<(String, bool)>>>;
    struct StringObserver {
        captured: Captured,
    }
    impl DataObserver for StringObserver {
        fn draw_string(&mut self, value: &str, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.to_string(), was_forced));
        }
    }
    let captured: Captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::String(vec![
        b'a' as u32,
        b'b' as u32,
        b'c' as u32,
    ])];
    let obs = Box::new(StringObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let intervals =
        crate::native::intervalsets::IntervalSet::new(vec![(0, 0xD7FF), (0xE000, 0x10FFFF)])
            .unwrap();
    let s = tc.draw_string(intervals.into(), 0, 10).ok().unwrap();
    assert_eq!(s, "abc");
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some(("abc".to_string(), false)));
}

#[test]
fn stop_span_extends_parent_label_stack() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    tc.start_span(1);
    tc.start_span(2);
    tc.stop_span(false);
    tc.stop_span(false);
}

#[test]
fn draw_float_unbounded_with_nan_can_produce_nan() {
    for seed in 0..200u64 {
        let mut tc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let v = tc
            .draw_float(f64::NEG_INFINITY, f64::INFINITY, true, true, 5e-324)
            .ok()
            .unwrap();
        if v.is_nan() {
            return;
        }
    }
    panic!("never produced NaN in 200 unbounded draws with allow_nan=true");
}

#[test]
fn draw_float_half_bounded_below_explores_finite_range() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let v = tc
        .draw_float(1.0, f64::INFINITY, false, false, 5e-324)
        .ok()
        .unwrap();
    assert!(v >= 1.0 && !v.is_nan());
}

#[test]
fn for_simplest_draws_integer_at_shrink_target_when_in_range() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let v = tc.draw_integer::<i128>(0, 23).ok().unwrap();
    assert_eq!(v, 0);
}

#[test]
fn for_simplest_draws_integer_clamped_to_range_when_target_below() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let v = tc.draw_integer::<i128>(5, 100).ok().unwrap();
    assert_eq!(v, 5);
}

#[test]
fn for_simplest_draws_integer_clamped_to_range_when_target_above() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let v = tc.draw_integer::<i128>(-100, -1).ok().unwrap();
    assert_eq!(v, -1);
}

#[test]
fn for_simplest_draws_float_at_zero() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let v = tc
        .draw_float(-10.0, 10.0, false, false, 5e-324)
        .ok()
        .unwrap();
    assert_eq!(v, 0.0);
    assert!(v.is_sign_positive(), "expected +0.0, got -0.0");
}

#[test]
fn for_simplest_draws_weighted_at_false() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let v = tc.weighted(0.5, None).ok().unwrap();
    assert!(!v, "weighted draw in simplest mode should be false");
}

#[test]
fn for_simplest_draws_bytes_at_min_size_all_zero() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let v = tc.draw_bytes(2, 5).ok().unwrap();
    assert_eq!(v, vec![0u8; 2], "expected min-sized all-zero buffer");
}

#[test]
fn for_simplest_is_independent_of_seed() {
    let mut a = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let mut b = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    for _ in 0..5 {
        let va = a.draw_integer::<i128>(0, 1000).ok().unwrap();
        let vb = b.draw_integer::<i128>(0, 1000).ok().unwrap();
        assert_eq!(va, vb);
        assert_eq!(va, 0);
    }
}

#[test]
fn for_simplest_records_choice_nodes() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE).unwrap();
    let _ = tc.draw_integer::<i128>(0, 23).ok().unwrap();
    let _ = tc.weighted(0.5, None).ok().unwrap();
    assert_eq!(tc.nodes.len(), 2);
}

#[test]
fn template_simplest_infinite_resolves_every_draw_to_simplest() {
    let mut tc = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(None).unwrap()),
        10,
        None,
    );
    for _ in 0..5 {
        assert_eq!(tc.draw_integer::<i128>(-100, 100).ok().unwrap(), 0);
    }
    assert!(!tc.weighted(0.5, None).ok().unwrap());
}

#[test]
fn template_simplest_finite_count_n_produces_exactly_n_values() {
    let mut tc = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(Some(3)).unwrap()),
        100,
        None,
    );
    for _ in 0..3 {
        assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
    }
    assert!(tc.draw_integer::<i128>(0, 100).is_err());
    assert_eq!(tc.status(), Some(Status::EarlyStop));
}

#[test]
fn template_concrete_prefix_then_template() {
    let prefix = vec![ChoiceValue::Integer(BigInt::from(42))];
    let mut tc = NativeTestCase::for_choices_and_template(
        &prefix,
        None,
        Some(ChoiceTemplate::simplest(None).unwrap()),
        10,
        None,
    );
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 42);
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
}

#[test]
fn template_concrete_prefix_with_punning_then_template() {
    let prefix = vec![ChoiceValue::Boolean(true)];
    let prefix_nodes = vec![ChoiceNode::boolean(BooleanChoice { p: 0.5 }, true, false)];
    let mut tc = NativeTestCase::for_choices_and_template(
        &prefix,
        Some(&prefix_nodes),
        Some(ChoiceTemplate::simplest(None).unwrap()),
        10,
        None,
    );
    let v = tc.draw_integer::<i128>(-100, 100).ok().unwrap();
    let expected_unit: i128 = IntegerChoice {
        min_value: BigInt::from(-100),
        max_value: BigInt::from(100),
        shrink_towards: BigInt::from(0),
    }
    .unit()
    .try_into()
    .unwrap();
    assert_eq!(v, expected_unit);
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
}

#[test]
fn template_count_zero_errors_at_construction() {
    let msg = ChoiceTemplate::simplest(Some(0)).unwrap_err().to_string();
    assert!(
        msg.contains("ChoiceTemplate count must be positive"),
        "{msg}"
    );
}

#[test]
fn for_simplest_wrapper_matches_template_with_count_none() {
    let mut a = NativeTestCase::for_simplest(5).unwrap();
    let mut b = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(None).unwrap()),
        5,
        None,
    );
    for _ in 0..5 {
        let va = a.draw_integer::<i128>(-10, 10).ok().unwrap();
        let vb = b.draw_integer::<i128>(-10, 10).ok().unwrap();
        assert_eq!(va, vb);
        assert_eq!(va, 0);
    }
}

#[test]
fn template_count_decrements_on_each_draw() {
    let mut tc = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(Some(3)).unwrap()),
        100,
        None,
    );
    for _ in 0..3 {
        let _ = tc.draw_integer::<i128>(0, 100).ok().unwrap();
    }
    assert_eq!(tc.trailing_template.as_ref().unwrap().count, Some(0));
    assert!(tc.draw_integer::<i128>(0, 100).is_err());
    assert_eq!(tc.trailing_template.as_ref().unwrap().count, Some(0));
}

/// Draw one full-width i64 sample under a fresh set of swarm parameters — the
/// aggregate marginal a caller sees across many test cases (each of which draws
/// its own parameters). Used by the distribution tests below.
fn swarm_sample(min: i128, max: i128, rng: &mut EngineRng) -> i128 {
    let params = GenerationParameters::draw(rng).unwrap();
    biased_i128_sample(min, max, rng, params).unwrap()
}

#[test]
fn biased_integer_sample_stays_in_range_for_small_bounds() {
    let mut rng = EngineRng::seeded(1);
    for _ in 0..1000 {
        let v = swarm_sample(0, 100, &mut rng);
        assert!((0..=100).contains(&v), "out of range: {v}");
    }
}

#[test]
fn biased_integer_sample_stays_in_range_for_wide_bounds() {
    let mut rng = EngineRng::seeded(2);
    for _ in 0..2000 {
        let v = swarm_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
        assert!(
            (i64::MIN as i128..=i64::MAX as i128).contains(&v),
            "out of range: {v}"
        );
    }
}

#[test]
fn biased_integer_sample_stays_in_range_for_full_i128() {
    let mut rng = EngineRng::seeded(3);
    for _ in 0..1000 {
        swarm_sample(i128::MIN, i128::MAX, &mut rng);
    }
}

#[test]
fn biased_integer_sample_collapses_when_min_equals_max() {
    let mut rng = EngineRng::seeded(4);
    for _ in 0..100 {
        assert_eq!(swarm_sample(42, 42, &mut rng), 42);
    }
}

#[test]
fn biased_integer_sample_produces_diverse_magnitudes_unbounded() {
    let mut rng = EngineRng::seeded(5);
    let mut magnitudes: HashSet<i32> = HashSet::default();
    for _ in 0..2000 {
        let v = swarm_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
        let mag = if v == 0 {
            0
        } else {
            128 - v.unsigned_abs().leading_zeros() as i32
        };
        magnitudes.insert(mag);
    }
    assert!(
        magnitudes.len() >= 10,
        "expected >= 10 magnitude buckets, got {}",
        magnitudes.len()
    );
}

#[test]
fn biased_integer_sample_concentrates_around_zero_when_unbounded() {
    let mut rng = EngineRng::seeded(6);
    let mut in_inner = 0;
    let total = 2000;
    for _ in 0..total {
        let v = swarm_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
        if v.unsigned_abs() <= 256 {
            in_inner += 1;
        }
    }
    let fraction = in_inner as f64 / total as f64;
    assert!(
        fraction > 0.05,
        "only {fraction} fraction in [-256, 256]; piecewise distribution not active"
    );
}

#[test]
fn biased_integer_sample_wide_range_still_draws_from_distribution() {
    let mut rng = EngineRng::seeded(8);
    let pool = &*SORTED_NASTY_POOL;
    let total = 2000;
    let mut outside_pool = 0;
    for _ in 0..total {
        let v = swarm_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
        if pool.binary_search(&v).is_err() {
            outside_pool += 1;
        }
    }
    let fraction = outside_pool as f64 / total as f64;
    assert!(
        fraction > 0.25,
        "only {fraction} of draws came from the distribution; nasty pool not capped?"
    );
}

#[test]
fn biased_integer_sample_log_skewed_bounded_range_favours_smaller_magnitudes() {
    let mut rng = EngineRng::seeded(11);
    let mut samples: Vec<i128> = (0..2000)
        .map(|_| swarm_sample(10_000, 10_000_000, &mut rng))
        .collect();
    samples.sort();
    let median = samples[samples.len() / 2];
    assert!(
        median < 1_000_000,
        "median {median} is too high; expected log-skewed distribution"
    );
}

/// Each category weight directly controls how often a wide draw returns a value
/// from that category: a high `endpoint_probability` makes the range edges
/// common, a high `interesting_probability` makes small magnitudes common, and
/// with every special weight at zero the endpoints (`min + 1` / `max - 1`, which
/// only the endpoint category produces) essentially vanish. Confirms the swarm
/// parameters are a pure reweighting of the same reachable values.
#[test]
fn biased_integer_sample_category_weights_control_the_mix() {
    let total = 200_000;
    let (lo, hi) = (i64::MIN as i128, i64::MAX as i128);

    let measure = |params: GenerationParameters, seed: u64| -> (f64, f64) {
        let mut rng = EngineRng::seeded(seed);
        let (mut endpoint, mut small) = (0u64, 0u64);
        for _ in 0..total {
            let v = biased_i128_sample(lo, hi, &mut rng, params).unwrap();
            if v == lo || v == hi || v == lo + 1 || v == hi - 1 {
                endpoint += 1;
            }
            if v.unsigned_abs() <= 8 {
                small += 1;
            }
        }
        (endpoint as f64 / total as f64, small as f64 / total as f64)
    };

    // Endpoint-heavy: the range edges dominate.
    let endpoint_heavy = GenerationParameters {
        endpoint_probability: 0.7,
        interesting_probability: 0.1,
        diffuse_probability: 0.05,
    };
    let (endpoint_rate, _) = measure(endpoint_heavy, 4242);
    assert!(
        endpoint_rate > 0.6,
        "at endpoint=0.7 endpoints only {endpoint_rate:.4}; expected > 60%"
    );

    // Interesting-heavy: small magnitudes become common.
    let interesting_heavy = GenerationParameters {
        endpoint_probability: 0.0,
        interesting_probability: 0.8,
        diffuse_probability: 0.0,
    };
    let (_, small_rate) = measure(interesting_heavy, 4243);
    assert!(
        small_rate > 0.1,
        "at interesting=0.8 small values only {small_rate:.4}; expected > 10%"
    );

    // All-middle: the `min + 1` / `max - 1` edges (unique to the endpoint
    // category) disappear.
    let all_middle = GenerationParameters {
        endpoint_probability: 0.0,
        interesting_probability: 0.0,
        diffuse_probability: 0.0,
    };
    let mut rng = EngineRng::seeded(4244);
    let mut inner_edges = 0u64;
    for _ in 0..total {
        let v = biased_i128_sample(lo, hi, &mut rng, all_middle).unwrap();
        if v == lo + 1 || v == hi - 1 {
            inner_edges += 1;
        }
    }
    let inner_edge_rate = inner_edges as f64 / total as f64;
    assert!(
        inner_edge_rate < 0.001,
        "with all special weights zero the inner edges still appear \
         {inner_edge_rate:.4} of the time; endpoint category not switched off"
    );
}

/// The heart of swarm testing: because a whole test case shares one set of
/// category weights, two operands drawn in the same case are *both* from the
/// interesting category far more often than when each operand draws its own
/// weights — even though the per-operand marginal is identical in both arms.
/// This positive correlation is what makes interactions that need several
/// special operands at once (e.g. `x + y` overflow) reachable; independent
/// per-operand weights reach them only at rate ~the product of the marginals.
#[test]
fn swarm_shared_parameters_correlate_operand_extremeness() {
    let (lo, hi) = (i64::MIN as i128, i64::MAX as i128);
    // Power-of-two values the interesting category draws directly but the other
    // sources essentially never land on *exactly*: they sit above the middle
    // distribution's `[-256, 256]` uniform core, below the diffuse pool's 2^16
    // floor, and the middle's heavy tail hits any specific integer with
    // vanishing probability. So "`v` is one of these" is effectively a pure
    // indicator that the interesting category fired — whose weight the shared
    // parameters control.
    const INTERESTING_ONLY: [i128; 10] = [
        512, -512, 1024, -1024, 2048, -2048, 4096, -4096, 8192, -8192,
    ];
    let interesting_hit = |v: i128| INTERESTING_ONLY.contains(&v);
    let pairs = 200_000;

    // `shared`: one parameter set per pair (both operands share the case's
    // mood). `independent`: a fresh parameter set per operand. Same per-operand
    // marginal; the only difference is the correlation `shared` introduces.
    let mut rng = EngineRng::seeded(2024);
    let (mut shared_both, mut independent_both) = (0u64, 0u64);
    for _ in 0..pairs {
        let params = GenerationParameters::draw(&mut rng).unwrap();
        let a = biased_i128_sample(lo, hi, &mut rng, params).unwrap();
        let b = biased_i128_sample(lo, hi, &mut rng, params).unwrap();
        if interesting_hit(a) && interesting_hit(b) {
            shared_both += 1;
        }
    }
    for _ in 0..pairs {
        let pa = GenerationParameters::draw(&mut rng).unwrap();
        let a = biased_i128_sample(lo, hi, &mut rng, pa).unwrap();
        let pb = GenerationParameters::draw(&mut rng).unwrap();
        let b = biased_i128_sample(lo, hi, &mut rng, pb).unwrap();
        if interesting_hit(a) && interesting_hit(b) {
            independent_both += 1;
        }
    }
    let shared = shared_both as f64 / pairs as f64;
    let independent = independent_both as f64 / pairs as f64;
    assert!(
        shared > independent * 1.4,
        "sharing the swarm parameters across operands should raise the \
         both-interesting rate well above independent draws (the E[p²] > E[p]² \
         effect): shared {shared:.4} vs independent {independent:.4}"
    );
}

/// `GenerationParameters::draw` (a Dirichlet over the four categories) must
/// produce valid, lumpy weights: each special weight in `[0, 1]` with their sum
/// `<= 1` (so the middle keeps positive probability), most cases middle-dominated
/// ("normal") with a near-zero endpoint weight, and a thin lumpy tail of
/// endpoint-heavy cases — the clustering that makes `x + y` overflow reachable.
#[test]
fn generation_parameters_draw_is_valid_lumpy_and_mostly_normal() {
    let mut rng = EngineRng::seeded(77);
    let total = 100_000;
    let (mut endpoint_heavy, mut endpoint_negligible, mut middle_dominant) = (0u64, 0u64, 0u64);
    for _ in 0..total {
        let p = GenerationParameters::draw(&mut rng).unwrap();
        for (name, v) in [
            ("endpoint", p.endpoint_probability),
            ("interesting", p.interesting_probability),
            ("diffuse", p.diffuse_probability),
        ] {
            assert!((0.0..=1.0).contains(&v), "{name} weight {v} out of [0, 1]");
        }
        let special = p.endpoint_probability + p.interesting_probability + p.diffuse_probability;
        assert!(special <= 1.0 + 1e-9, "special mass {special} exceeds 1");
        let middle = 1.0 - special;
        if p.endpoint_probability > 0.5 {
            endpoint_heavy += 1;
        }
        if p.endpoint_probability < 0.01 {
            endpoint_negligible += 1;
        }
        if middle > 0.5 {
            middle_dominant += 1;
        }
    }
    assert!(
        endpoint_heavy > 0,
        "no endpoint-heavy cases; the lumpy tail that drives overflow is missing"
    );
    assert!(
        endpoint_negligible as f64 / total as f64 > 0.5,
        "endpoint weight is near zero in only {}/{total} cases; expected most \
         cases to have essentially no endpoints (lumpiness)",
        endpoint_negligible
    );
    assert!(
        middle_dominant as f64 / total as f64 > 0.5,
        "the middle dominates in only {}/{total} cases; expected most cases to \
         be normal",
        middle_dominant
    );
}

/// `sample_gamma` must have the Gamma distribution's mean and variance (both
/// equal to the shape), across both the `shape >= 1` path and the `shape < 1`
/// boost path.
#[test]
fn sample_gamma_matches_distribution_moments() {
    let n = 200_000;
    for shape in [0.3_f64, 1.0, 2.5] {
        let mut rng = EngineRng::seeded(1000 + (shape * 10.0) as u64);
        let mut sum = 0.0;
        let mut sum_sq = 0.0;
        for _ in 0..n {
            let g = sample_gamma(shape, &mut rng).unwrap();
            assert!(g >= 0.0 && g.is_finite(), "gamma produced {g}");
            sum += g;
            sum_sq += g * g;
        }
        let mean = sum / n as f64;
        let var = sum_sq / n as f64 - mean * mean;
        assert!(
            (mean - shape).abs() < 0.05 * shape.max(1.0),
            "Gamma({shape}) mean {mean:.4}; expected ~{shape}"
        );
        assert!(
            (var - shape).abs() < 0.1 * shape.max(1.0),
            "Gamma({shape}) variance {var:.4}; expected ~{shape}"
        );
    }
}

/// `sample_dirichlet4` must return a point on the simplex (weights in `[0, 1]`
/// summing to 1) whose component means match the normalised concentrations.
#[test]
fn sample_dirichlet4_lands_on_simplex_with_right_means() {
    let alphas = [0.08_f64, 0.8, 0.12, 2.2];
    let total_alpha: f64 = alphas.iter().sum();
    let n = 200_000;
    let mut rng = EngineRng::seeded(9999);
    let mut sums = [0.0_f64; 4];
    for _ in 0..n {
        let w = sample_dirichlet4(alphas, &mut rng).unwrap();
        let s: f64 = w.iter().sum();
        assert!((s - 1.0).abs() < 1e-9, "weights sum to {s}, not 1");
        for (acc, &wi) in sums.iter_mut().zip(w.iter()) {
            assert!((0.0..=1.0).contains(&wi), "weight {wi} out of [0, 1]");
            *acc += wi;
        }
    }
    for (i, (&acc, &alpha)) in sums.iter().zip(alphas.iter()).enumerate() {
        let mean = acc / n as f64;
        let expected = alpha / total_alpha;
        assert!(
            (mean - expected).abs() < 0.01,
            "category {i} mean {mean:.4}; expected {expected:.4}"
        );
    }
}

/// `normalize_to_simplex` scales positive weights to sum to 1, and falls back
/// to an even split when every weight is zero (the all-underflowed guard).
#[test]
fn normalize_to_simplex_scales_and_handles_all_zero() {
    let n = normalize_to_simplex([1.0, 3.0, 0.0, 0.0]);
    assert!((n.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!((n[0] - 0.25).abs() < 1e-12 && (n[1] - 0.75).abs() < 1e-12);
    assert_eq!(n[2], 0.0);

    assert_eq!(normalize_to_simplex([0.0; 4]), [0.25; 4]);
}

#[test]
fn biased_string_sample_caps_constant_pool_probability() {
    let sc = StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(vec![
            (0, 0xD7FF),
            (0xE000, 0x10FFFF),
        ])
        .unwrap()
        .into(),
        min_size: 0,
        max_size: 100,
    };
    let mut rng = EngineRng::seeded(9);
    let pool = &*GLOBAL_CONSTANTS_STRINGS;
    let total = 2000;
    let mut from_pool = 0;
    for _ in 0..total {
        let v = biased_string_sample(&sc, &mut rng).unwrap();
        if pool.contains(&v) {
            from_pool += 1;
        }
    }
    let fraction = from_pool as f64 / total as f64;
    assert!(
        fraction < 0.56,
        "{fraction} of draws came from the constant pool; threshold not capped?"
    );
}

#[test]
fn biased_string_sample_empty_alphabet_returns_empty_string() {
    let sc = StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(vec![])
            .unwrap()
            .into(),
        min_size: 0,
        max_size: 0,
    };
    let mut rng = EngineRng::seeded(7);
    for _ in 0..200 {
        assert_eq!(
            biased_string_sample(&sc, &mut rng).unwrap(),
            Vec::<u32>::new()
        );
    }
}

#[test]
fn biased_float_sample_full_finite_range_does_not_collapse_to_max() {
    let fc = FloatChoice {
        min_value: -f64::MAX,
        max_value: f64::MAX,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: 5e-324,
    };
    let mut rng = EngineRng::seeded(10);
    let total = 2000;
    let mut at_max = 0;
    let mut integral = 0;
    for _ in 0..total {
        let v = biased_float_sample(&fc, &mut rng).unwrap();
        assert!(v.is_finite(), "drew non-finite {v}");
        if v.abs() == f64::MAX {
            at_max += 1;
        }
        if v == v.trunc() {
            integral += 1;
        }
    }
    let max_fraction = at_max as f64 / total as f64;
    assert!(
        max_fraction < 0.2,
        "{max_fraction} of draws were ±f64::MAX; range-width overflow regressed?"
    );
    let integral_fraction = integral as f64 / total as f64;
    assert!(
        integral_fraction > 0.2,
        "only {integral_fraction} of draws were integer-valued; lex bias missing?"
    );
}

#[test]
fn biased_integer_sample_narrow_range_uses_uniform_fallback() {
    let mut rng = EngineRng::seeded(7);
    let mut seen_zero = false;
    let mut seen_one = false;
    for _ in 0..200 {
        let params = GenerationParameters::draw(&mut rng).unwrap();
        let v = biased_i128_sample(0, 1, &mut rng, params).unwrap();
        assert!((0..=1).contains(&v), "out of range: {v}");
        match v {
            0 => seen_zero = true,
            1 => seen_one = true,
            _ => unreachable!(),
        }
        if seen_zero && seen_one {
            break;
        }
    }
    assert!(seen_zero && seen_one);
}

/// The erased entry point uses BigInt; a small range fits the i128
/// fast path and must produce values in range.
#[test]
fn biased_integer_sample_erased_small_width_stays_in_range() {
    let kind = IntegerChoice {
        min_value: BigInt::from(0u8),
        max_value: BigInt::from(200u8),
        shrink_towards: BigInt::from(0u8),
    };
    let mut rng = EngineRng::seeded(21);
    for _ in 0..500 {
        let params = GenerationParameters::draw(&mut rng).unwrap();
        let v = biased_integer_sample(&kind, &mut rng, params).unwrap();
        assert!(kind.validate(&v), "out of range: {v:?}");
    }
}

/// A `BigInt` choice whose span exceeds `i128` exercises the big-range
/// sampler (`biguint_sample_in_range`) and its nasty pool.
#[test]
fn biased_integer_sample_erased_bigint_beyond_i128_stays_in_range() {
    let min = BigInt::from(i128::MIN) * BigInt::from(1_000_000);
    let max = BigInt::from(i128::MAX) * BigInt::from(1_000_000);
    let kind = IntegerChoice {
        min_value: min,
        max_value: max,
        shrink_towards: BigInt::from(0),
    };
    let mut rng = EngineRng::seeded(22);
    for _ in 0..500 {
        let params = GenerationParameters::draw(&mut rng).unwrap();
        let v = biased_integer_sample(&kind, &mut rng, params).unwrap();
        assert!(kind.validate(&v), "out of range: {v:?}");
    }
}

#[test]
fn integer_sample_from_distribution_uniform_fallback_for_indistinguishable_bounds() {
    let mut rng = EngineRng::seeded(13);
    let min = i128::MAX - 1000;
    let max = i128::MAX;
    let mut all_endpoints = true;
    for _ in 0..50 {
        let v = integer_sample_from_distribution(min, max, &mut rng).unwrap();
        assert!(v >= min && v <= max, "out of range: {v}");
        if v != min && v != max {
            all_endpoints = false;
        }
    }
    assert!(
        !all_endpoints,
        "uniform fallback should produce values across the range"
    );
}

/// A `BigInt` choice with `min == max` beyond i128 collapses to that single
/// value (the `biguint_sample_in_range` early return).
#[test]
fn biased_integer_sample_erased_bigint_single_value() {
    let fixed = BigInt::from(i128::MAX) * BigInt::from(1_000_000);
    let kind = IntegerChoice {
        min_value: fixed.clone(),
        max_value: fixed.clone(),
        shrink_towards: BigInt::from(0),
    };
    let mut rng = EngineRng::seeded(23);
    for _ in 0..20 {
        let params = GenerationParameters::draw(&mut rng).unwrap();
        assert_eq!(
            biased_integer_sample(&kind, &mut rng, params).unwrap(),
            fixed.clone()
        );
    }
}

/// The weighted-boolean draw must spend exactly one byte of entropy
/// (Hypothesis's `BytestringProvider` approach), not a full `f64`. The urandom
/// backend feeds every byte from the fuzzer, so a one-bit decision must cost
/// one byte. Regression for an earlier `rng.random::<f64>() <= p` that burned
/// eight bytes per boolean.
#[test]
fn weighted_boolean_sample_consumes_exactly_one_byte() {
    use rand::Rng;
    let mut a = EngineRng::seeded(12345);
    let mut b = EngineRng::seeded(12345);
    let result = weighted_boolean_sample(0.5, &mut a);
    let mut byte = [0u8; 1];
    b.fill_bytes(&mut byte);
    let falsey = (256.0_f64 * (1.0 - 0.5)).floor().max(1.0) as u32;
    assert_eq!(result, u32::from(byte[0]) >= falsey);
    assert_eq!(a.next_u64(), b.next_u64());
}

/// `p` still controls the probability of `true` under the byte-based draw.
#[test]
fn weighted_boolean_sample_respects_probability() {
    let mut rng = EngineRng::seeded(99);
    let n = 5000usize;
    let high = (0..n)
        .filter(|_| weighted_boolean_sample(0.9, &mut rng))
        .count();
    let low = (0..n)
        .filter(|_| weighted_boolean_sample(0.1, &mut rng))
        .count();
    assert!(high > n * 3 / 4, "p=0.9 produced only {high}/{n} trues");
    assert!(low < n / 4, "p=0.1 produced {low}/{n} trues");
}

#[test]
fn float_clamp_reroutes_excluded_magnitude_band() {
    let fc = FloatChoice {
        min_value: -1e-307,
        max_value: 1e-307,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    let raw = f64::from_bits(((1u64 << 52) - 1) / 2);
    let clamped = float_clamp(&fc, raw);
    assert_eq!(clamped, f64::MIN_POSITIVE);

    let fc_neg = FloatChoice {
        min_value: -1e-307,
        max_value: -1e-308,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    let raw_neg = f64::from_bits((((1u64 << 52) - 1) / 10) * 9);
    let clamped_neg = float_clamp(&fc_neg, raw_neg);
    assert_eq!(clamped_neg, -f64::MIN_POSITIVE);
}

#[test]
fn float_clamp_with_infinite_bounds_stays_finite() {
    let fc = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: true,
        smallest_nonzero_magnitude: f64::from(f32::from_bits(1)),
    };
    for raw in [5e-324, 1e-100, -3e-320, f64::from_bits(12345)] {
        let clamped = float_clamp(&fc, raw);
        assert!(
            clamped.is_finite(),
            "float_clamp({raw:e}) produced {clamped}"
        );
    }
}

#[test]
fn draw_string_with_inverted_sizes_is_an_internal_error() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    let intervals = crate::native::intervalsets::IntervalSet::new(vec![(0, 0xD7FF)]).unwrap();
    let msg = tc
        .draw_string(intervals.into(), 5, 4)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("min_size <= max_size"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn draw_string_empty_alphabet_zero_max_size_draws_empty_string() {
    let choices = vec![ChoiceValue::String(Vec::new())];
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    let intervals = crate::native::intervalsets::IntervalSet::new(vec![]).unwrap();
    let s = tc.draw_string(intervals.into(), 0, 0).ok().unwrap();
    assert_eq!(s, "");
}

#[test]
fn draw_string_empty_alphabet_zero_max_size_puns_invalid_prefix_to_empty() {
    let choices = vec![ChoiceValue::Integer(crate::native::bignum::BigInt::from(5))];
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    let intervals = crate::native::intervalsets::IntervalSet::new(vec![]).unwrap();
    let s = tc.draw_string(intervals.into(), 0, 0).ok().unwrap();
    assert_eq!(s, "");
}

#[test]
fn draw_string_with_empty_alphabet_and_nonzero_max_is_an_internal_error() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    let intervals = crate::native::intervalsets::IntervalSet::new(vec![]).unwrap();
    let msg = tc
        .draw_string(intervals.into(), 0, 4)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("empty alphabet"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn weighted_boolean_sample_keeps_true_reachable_for_tiny_p() {
    let mut rng = EngineRng::seeded(3);
    let trues = (0..20_000)
        .filter(|_| weighted_boolean_sample(1e-300, &mut rng))
        .count();
    assert!(trues > 0, "true must stay reachable for any p > 0");
    assert!(trues < 400, "tiny p must stay rare, got {trues}/20000");
}

#[test]
fn spans_trivial_returns_false_for_a_stale_out_of_range_span() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 5,
        end: 7,
        label: "stale".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert!(
        !spans.trivial(0, &[]).unwrap(),
        "a span past the end of the nodes must not count as trivial"
    );
}

fn fresh_id_kind_max(tc: &NativeTestCase, i: usize) -> BigInt {
    use crate::native::core::choices::ChoiceKind;
    match tc.nodes[i].kind() {
        ChoiceKind::Integer(ic) => ic.max_value.clone(),
        other => panic!("expected an integer kind, got {other:?}"),
    }
}

#[test]
fn draw_fresh_id_hands_out_sequential_ids_during_generation() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    assert_eq!(tc.draw_fresh_id().unwrap(), 1);
    assert_eq!(tc.draw_fresh_id().unwrap(), 2);
    assert!(tc.nodes.iter().all(|n| !n.was_forced));
    assert_eq!(fresh_id_kind_max(&tc, 0), BigInt::from(1));
    assert_eq!(fresh_id_kind_max(&tc, 1), BigInt::from(2));
    assert_eq!(fresh_id_kind_max(&tc, 2), BigInt::from(3));
}

#[test]
fn draw_fresh_id_keeps_the_hole_when_the_first_addition_is_deleted() {
    let choices = [
        ChoiceValue::Integer(BigInt::from(1)),
        ChoiceValue::Integer(BigInt::from(2)),
        ChoiceValue::Integer(BigInt::from(3)),
    ];
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    assert_eq!(tc.draw_fresh_id().unwrap(), 1);
    assert_eq!(tc.draw_fresh_id().unwrap(), 2);
    assert_eq!(tc.draw_fresh_id().unwrap(), 3);
}

#[test]
fn draw_fresh_id_keeps_the_hole_when_a_middle_addition_is_deleted() {
    let choices = [
        ChoiceValue::Integer(BigInt::from(0)),
        ChoiceValue::Integer(BigInt::from(2)),
    ];
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    assert_eq!(tc.draw_fresh_id().unwrap(), 2);
}

/// With a `high + 1` bound the first surviving id after a deletion would sit
/// just outside the empty-registry window `[0, 0]`, get punned to `0`, and
/// renumber every later survivor. The `+ 2` headroom keeps the whole
/// suffix intact.
#[test]
fn draw_fresh_id_does_not_cascade_after_a_single_deletion() {
    let choices = [
        ChoiceValue::Integer(BigInt::from(1)),
        ChoiceValue::Integer(BigInt::from(2)),
        ChoiceValue::Integer(BigInt::from(3)),
        ChoiceValue::Integer(BigInt::from(4)),
    ];
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    let ids: Vec<i64> = (0..4).map(|_| tc.draw_fresh_id().unwrap()).collect();
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

/// Deleting two adjacent additions leaves a gap of three; the survivor falls
/// outside the window and is repaired to the smallest unused id. Accepted
/// shrinks re-record realized values, so the repair does not recur.
#[test]
fn draw_fresh_id_repairs_a_gap_of_three_to_small_ids() {
    let choices = [
        ChoiceValue::Integer(BigInt::from(2)),
        ChoiceValue::Integer(BigInt::from(3)),
    ];
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    assert_eq!(tc.draw_fresh_id().unwrap(), 1);
}

#[test]
fn draw_fresh_id_repairs_a_used_prefix_id_to_the_smallest_unused() {
    let choices = [
        ChoiceValue::Integer(BigInt::from(0)),
        ChoiceValue::Integer(BigInt::from(0)),
        ChoiceValue::Integer(BigInt::from(1)),
    ];
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    assert_eq!(tc.draw_fresh_id().unwrap(), 1);
    assert_eq!(tc.draw_fresh_id().unwrap(), 2);
}

#[test]
fn draw_fresh_id_fills_gaps_when_generating_past_the_prefix() {
    let choices = [ChoiceValue::Integer(BigInt::from(1))];
    let mut tc = NativeTestCase::for_choices_and_template(&choices, None, None, BUFFER_SIZE, None)
        .with_random(EngineRng::seeded(0))
        .unwrap();
    assert_eq!(tc.draw_fresh_id().unwrap(), 1);
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    assert_eq!(tc.draw_fresh_id().unwrap(), 2);
    assert_eq!(tc.draw_fresh_id().unwrap(), 3);
}

/// The window is anchored on the family registry, so a clone stream
/// continues where the parent's ids left off instead of colliding and
/// being repaired: ids stay family-unique and every window admits the
/// smallest unused id.
#[test]
fn draw_fresh_id_continues_across_clone_streams_without_collisions() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    assert_eq!(parent.draw_fresh_id().unwrap(), 0);
    assert_eq!(parent.draw_fresh_id().unwrap(), 1);
    assert_eq!(parent.draw_fresh_id().unwrap(), 2);
    let child = parent.clone_stream().unwrap();
    let mut child_ntc = child.lock();
    assert_eq!(child_ntc.draw_fresh_id().unwrap(), 3);
    assert_eq!(fresh_id_kind_max(&child_ntc, 0), BigInt::from(4));
    assert_eq!(child_ntc.draw_fresh_id().unwrap(), 4);
}

#[test]
fn draw_fresh_id_puns_a_mismatched_prefix_kind() {
    let mut tc = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None, None);
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
}

#[test]
fn draw_fresh_id_notifies_the_observer() {
    use std::sync::{Arc, Mutex};
    struct IdObserver {
        captured: Arc<Mutex<Option<(BigInt, bool)>>>,
    }
    impl DataObserver for IdObserver {
        fn draw_integer(&mut self, value: &BigInt, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.clone(), was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let obs = Box::new(IdObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices_and_template(&[], None, None, 4, Some(obs))
        .with_random(EngineRng::seeded(0))
        .unwrap();
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((BigInt::from(0), false)));
}

#[test]
fn smallest_unused_id_skips_gaps() {
    let mut used = BTreeSet::new();
    assert_eq!(smallest_unused_id(&used), 0);
    used.insert(1);
    assert_eq!(smallest_unused_id(&used), 0);
    used.insert(0);
    used.insert(3);
    assert_eq!(smallest_unused_id(&used), 2);
    used.insert(2);
    assert_eq!(smallest_unused_id(&used), 4);
}

#[test]
fn draw_from_set_generates_a_member_and_records_it_by_value() {
    let members = [4, 9];
    for seed in 0..10 {
        let mut tc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        for i in 0..10 {
            assert_eq!(tc.draw_fresh_id().unwrap(), i);
        }
        let chosen = tc.draw_from_set(&members).unwrap();
        assert!(members.contains(&chosen));
        assert_eq!(
            tc.nodes.last().unwrap().value(),
            ChoiceValue::Integer(BigInt::from(chosen))
        );
        assert_eq!(fresh_id_kind_max(&tc, tc.nodes.len() - 1), BigInt::from(10));
    }
}

fn tc_with_fresh_ids(count: i64, tail: &[i64]) -> NativeTestCase {
    let mut choices: Vec<ChoiceValue> = (0..count)
        .map(|i| ChoiceValue::Integer(BigInt::from(i)))
        .collect();
    choices.extend(tail.iter().map(|&v| ChoiceValue::Integer(BigInt::from(v))));
    let mut tc = NativeTestCase::for_choices(&choices, None, None);
    for i in 0..count {
        assert_eq!(tc.draw_fresh_id().unwrap(), i);
    }
    tc
}

#[test]
fn draw_from_set_replays_a_surviving_member() {
    let mut tc = tc_with_fresh_ids(3, &[1]);
    assert_eq!(tc.draw_from_set(&[0, 1, 2]).unwrap(), 1);
}

#[test]
fn draw_from_set_repairs_a_dead_value_to_the_largest_member_below() {
    let mut tc = tc_with_fresh_ids(3, &[1]);
    assert_eq!(tc.draw_from_set(&[0, 2]).unwrap(), 0);
    assert_eq!(
        tc.nodes.last().unwrap().value(),
        ChoiceValue::Integer(BigInt::from(0))
    );
}

#[test]
fn draw_from_set_repairs_a_value_below_all_members_to_the_smallest() {
    let mut tc = tc_with_fresh_ids(3, &[0]);
    assert_eq!(tc.draw_from_set(&[1, 2]).unwrap(), 1);
}

/// A reference just above the window (its addition was deleted along with
/// everything after it) fails validation and puns to the smallest member.
#[test]
fn draw_from_set_puns_a_value_beyond_the_window_to_the_smallest_member() {
    let mut tc = tc_with_fresh_ids(2, &[4]);
    assert_eq!(tc.draw_from_set(&[0, 1]).unwrap(), 0);
}

#[test]
fn draw_from_set_accepts_unsorted_duplicated_members() {
    let mut tc = tc_with_fresh_ids(3, &[1]);
    assert_eq!(tc.draw_from_set(&[2, 1, 1, 0]).unwrap(), 1);
}

#[test]
fn draw_from_set_notifies_the_observer() {
    use std::sync::{Arc, Mutex};
    struct SetObserver {
        captured: Arc<Mutex<Option<(BigInt, bool)>>>,
    }
    impl DataObserver for SetObserver {
        fn draw_integer(&mut self, value: &BigInt, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.clone(), was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let obs = Box::new(SetObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices_and_template(&[], None, None, 4, Some(obs))
        .with_random(EngineRng::seeded(0))
        .unwrap();
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    let chosen = tc.draw_from_set(&[0]).unwrap();
    assert_eq!(chosen, 0);
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((BigInt::from(0), false)));
}

#[test]
fn draw_from_set_with_no_members_is_an_internal_error() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    assert!(tc.draw_from_set(&[]).is_err());
}

#[test]
fn draw_from_set_with_negative_members_is_an_internal_error() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    assert!(tc.draw_from_set(&[-1, 3]).is_err());
}

#[test]
fn draw_from_set_with_members_beyond_the_registry_is_an_internal_error() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    assert_eq!(tc.draw_fresh_id().unwrap(), 0);
    assert!(tc.draw_from_set(&[7]).is_err());
}

#[test]
fn native_variables_add_active_and_consume_round_trip() {
    let mut vars = NativeVariables::new();
    vars.add(0);
    vars.add(1);
    vars.add(2);
    assert_eq!(vars.active(), vec![0, 1, 2]);
    vars.consume(1);
    assert_eq!(vars.active(), vec![0, 2]);
    vars.consume(2);
    assert_eq!(vars.active(), vec![0]);
    vars.consume(0);
    assert_eq!(vars.active(), Vec::<i64>::new());
}
