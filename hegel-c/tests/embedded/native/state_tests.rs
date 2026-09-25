use super::float_sample::*;
use super::*;
use crate::native::HashSet;
use crate::native::core::BUFFER_SIZE;
use crate::native::core::GenerationParameters;
use crate::native::core::choices::{BooleanChoice, ChoiceKind};
use crate::native::rng::EngineRng;
use alloc::string::ToString;

#[test]
fn spans_get_mut_returns_mutable_reference() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: 1,
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
        label: 1,
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
        label: 1,
        depth: 0,
        parent: None,
        discarded: false,
    });
    let v = spans.into_vec();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].label, 1);
}

#[test]
fn spans_from_vec() {
    let v = vec![Span {
        start: 0,
        end: 3,
        label: 1,
        depth: 0,
        parent: None,
        discarded: false,
    }];
    let spans = Spans::from(v);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0usize].label, 1);
}

#[test]
fn spans_deref_to_slice() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: 1,
        depth: 0,
        parent: None,
        discarded: false,
    });
    let slice: &[Span] = &spans;
    assert_eq!(slice.len(), 1);
    assert_eq!(slice[0].label, 1);
}

#[test]
fn spans_into_iterator() {
    let mut spans = Spans::new();
    for i in 0..3 {
        spans.push(Span {
            start: i,
            end: i + 1,
            label: i as u64,
            depth: 0,
            parent: None,
            discarded: false,
        });
    }
    let labels: Vec<u64> = (&spans).into_iter().map(|s| s.label).collect();
    assert_eq!(labels, vec![0, 1, 2]);
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
        label: 1,
        depth: 0,
        parent: None,
        discarded: false,
    });
    spans.push(Span {
        start: 1,
        end: 2,
        label: 2,
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert_eq!(spans.get(0).unwrap().label, 1);
    assert_eq!(spans.get(1).unwrap().label, 2);
    assert!(spans.get(2).is_none());
}

#[test]
fn spans_as_slice_returns_slice() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: 1,
        depth: 0,
        parent: None,
        discarded: false,
    });
    let sl = spans.as_slice();
    assert_eq!(sl.len(), 1);
    assert_eq!(sl[0].label, 1);
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
    let v = tc
        .draw_float(FloatWidth::F64, 0.0, 10.0, false, false, 5e-324)
        .ok()
        .unwrap();
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
fn stop_span_closes_nested_spans_innermost_first() {
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
            .draw_float(
                FloatWidth::F64,
                f64::NEG_INFINITY,
                f64::INFINITY,
                true,
                true,
                5e-324,
            )
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
        .draw_float(FloatWidth::F64, 1.0, f64::INFINITY, false, false, 5e-324)
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
        .draw_float(FloatWidth::F64, -10.0, 10.0, false, false, 5e-324)
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
    let params = GenerationParameters::draw(rng).unwrap().integer;
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

    let measure = |params: IntegerGenerationParameters, seed: u64| -> (f64, f64) {
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
    let endpoint_heavy = IntegerGenerationParameters {
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
    let interesting_heavy = IntegerGenerationParameters {
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
    let all_middle = IntegerGenerationParameters {
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
        let params = GenerationParameters::draw(&mut rng).unwrap().integer;
        let a = biased_i128_sample(lo, hi, &mut rng, params).unwrap();
        let b = biased_i128_sample(lo, hi, &mut rng, params).unwrap();
        if interesting_hit(a) && interesting_hit(b) {
            shared_both += 1;
        }
    }
    for _ in 0..pairs {
        let pa = GenerationParameters::draw(&mut rng).unwrap().integer;
        let a = biased_i128_sample(lo, hi, &mut rng, pa).unwrap();
        let pb = GenerationParameters::draw(&mut rng).unwrap().integer;
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
        let p = GenerationParameters::draw(&mut rng).unwrap().integer;
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

fn float_weights(f: &FloatGenerationParameters) -> [f64; 17] {
    [
        f.endpoint_probability,
        f.near_zero_probability,
        f.subnormal_probability,
        f.near_one_probability,
        f.integer_probability,
        f.half_integer_probability,
        f.near_max_for_add_probability,
        f.near_max_for_mul_probability,
        f.near_sqrt_min_positive_probability,
        f.nan_probability,
        f.infinity_probability,
        f.max_magnitude_probability,
        f.max_exact_integer_probability,
        f.signed_zero_probability,
        f.binade_edge_probability,
        f.non_dyadic_probability,
        f.default_probability,
    ]
}

/// The float weights come from their own `DIRICHLET_ALPHA_FLOAT_*` concentrations
/// — defaults at those means, draws on the simplex — and are drawn independently
/// of the integer weights, so a case's float mix is not a copy of its integer mix.
#[test]
fn float_generation_parameters_follow_their_own_alphas_and_draw_independently() {
    let int_default = IntegerGenerationParameters::default();
    let float_default = FloatGenerationParameters::default();
    let alphas = [
        DIRICHLET_ALPHA_FLOAT_ENDPOINT,
        DIRICHLET_ALPHA_FLOAT_NEAR_ZERO,
        DIRICHLET_ALPHA_FLOAT_SUBNORMAL,
        DIRICHLET_ALPHA_FLOAT_NEAR_ONE,
        DIRICHLET_ALPHA_FLOAT_INTEGER,
        DIRICHLET_ALPHA_FLOAT_HALF_INTEGER,
        DIRICHLET_ALPHA_FLOAT_NEAR_MAX_FOR_ADD,
        DIRICHLET_ALPHA_FLOAT_NEAR_MAX_FOR_MUL,
        DIRICHLET_ALPHA_FLOAT_NEAR_SQRT_MIN_POSITIVE,
        DIRICHLET_ALPHA_FLOAT_NAN,
        DIRICHLET_ALPHA_FLOAT_INFINITY,
        DIRICHLET_ALPHA_FLOAT_MAX_MAGNITUDE,
        DIRICHLET_ALPHA_FLOAT_MAX_EXACT_INTEGER,
        DIRICHLET_ALPHA_FLOAT_SIGNED_ZERO,
        DIRICHLET_ALPHA_FLOAT_BINADE_EDGE,
        DIRICHLET_ALPHA_FLOAT_NON_DYADIC,
        DIRICHLET_ALPHA_FLOAT_DEFAULT,
    ];
    let alpha_total: f64 = alphas.iter().sum();
    let defaults = float_weights(&float_default);
    for (i, (&w, &alpha)) in defaults.iter().zip(alphas.iter()).enumerate() {
        assert_eq!(w, alpha / alpha_total, "float default weight {i}");
    }
    assert_eq!(
        GenerationParameters::default(),
        GenerationParameters {
            integer: int_default,
            float: float_default,
        }
    );

    let mut rng = EngineRng::seeded(78);
    let total = 10_000;
    let mut differ = 0u64;
    let mut float_endpoint_sum = 0.0;
    for _ in 0..total {
        let p = GenerationParameters::draw(&mut rng).unwrap();
        let f = p.float;
        let weights = float_weights(&f);
        for (i, &v) in weights.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(&v),
                "float weight {i} = {v} out of [0, 1]"
            );
        }
        let mass: f64 = weights.iter().sum();
        assert!(
            (mass - 1.0).abs() <= 1e-9,
            "float weights sum to {mass}, expected 1"
        );
        if f.endpoint_probability != p.integer.endpoint_probability {
            differ += 1;
        }
        float_endpoint_sum += f.endpoint_probability;
    }
    assert_eq!(
        differ,
        total,
        "float and integer endpoint weights coincided in {} cases; expected \
         independent draws",
        total - differ
    );
    let float_endpoint_mean = float_endpoint_sum / total as f64;
    assert!(
        (float_endpoint_mean - float_default.endpoint_probability).abs() < 0.01,
        "float endpoint mean {float_endpoint_mean:.4} far from the Dirichlet mean \
         {:.4}",
        float_default.endpoint_probability
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

/// `sample_dirichlet` must return a point on the simplex (weights in `[0, 1]`
/// summing to 1) whose component means match the normalised concentrations.
#[test]
fn sample_dirichlet_lands_on_simplex_with_right_means() {
    let alphas = [0.08_f64, 0.8, 0.12, 2.2];
    let total_alpha: f64 = alphas.iter().sum();
    let n = 200_000;
    let mut rng = EngineRng::seeded(9999);
    let mut sums = [0.0_f64; 4];
    for _ in 0..n {
        let w = sample_dirichlet(alphas, &mut rng).unwrap();
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
fn constants_in_alphabet_is_memoised_on_the_interval_set() {
    let intervals =
        crate::native::intervalsets::IntervalSet::new(vec![('a' as u32, 'z' as u32)]).unwrap();
    assert!(intervals.string_constants_mask.get().is_none());

    let mask = constants_in_alphabet(&intervals);
    assert_eq!(mask.len(), GLOBAL_CONSTANTS_STRINGS.len());
    for (cps, &contained) in GLOBAL_CONSTANTS_STRINGS.iter().zip(mask) {
        assert_eq!(contained, cps.iter().all(|&cp| intervals.contains(cp)));
    }
    assert!(mask.iter().any(|&m| m));
    assert!(mask.iter().any(|&m| !m));

    let again = constants_in_alphabet(&intervals);
    assert!(core::ptr::eq(mask.as_ptr(), again.as_ptr()));
    assert!(intervals.string_constants_mask.get().is_some());

    let other =
        crate::native::intervalsets::IntervalSet::new(vec![('A' as u32, 'Z' as u32)]).unwrap();
    assert!(other.string_constants_mask.get().is_none());
    assert_ne!(constants_in_alphabet(&other), mask);
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
        let v = biased_float_sample(
            &fc,
            FloatWidth::F64,
            &mut rng,
            FloatGenerationParameters::default(),
        )
        .unwrap();
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
        "only {integral_fraction} of draws were integer-valued; default draw missing?"
    );
}

#[test]
fn biased_integer_sample_narrow_range_uses_uniform_fallback() {
    let mut rng = EngineRng::seeded(7);
    let mut seen_zero = false;
    let mut seen_one = false;
    for _ in 0..200 {
        let params = GenerationParameters::draw(&mut rng).unwrap().integer;
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
        let params = GenerationParameters::draw(&mut rng).unwrap().integer;
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
        let params = GenerationParameters::draw(&mut rng).unwrap().integer;
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
        let params = GenerationParameters::draw(&mut rng).unwrap().integer;
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
fn float_restrict_and_redraw_reroutes_excluded_magnitude_band() {
    let fc = FloatChoice {
        min_value: -1e-307,
        max_value: 1e-307,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    let raw = f64::from_bits(((1u64 << 52) - 1) / 2);
    let clamped = float_restrict_and_redraw(&fc, raw);
    assert_eq!(clamped, f64::MIN_POSITIVE);

    let fc_neg = FloatChoice {
        min_value: -1e-307,
        max_value: -1e-308,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    let raw_neg = f64::from_bits((((1u64 << 52) - 1) / 10) * 9);
    let clamped_neg = float_restrict_and_redraw(&fc_neg, raw_neg);
    assert_eq!(clamped_neg, -f64::MIN_POSITIVE);
}

#[test]
fn float_restrict_and_redraw_with_infinite_bounds_stays_finite() {
    let fc = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: true,
        smallest_nonzero_magnitude: f64::from(f32::from_bits(1)),
    };
    for raw in [5e-324, 1e-100, -3e-320, f64::from_bits(12345)] {
        let clamped = float_restrict_and_redraw(&fc, raw);
        assert!(
            clamped.is_finite(),
            "float_restrict_and_redraw({raw:e}) produced {clamped}"
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
        label: 1,
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
fn fresh_ids_track_the_smallest_unused_id_across_gaps() {
    let mut used = FreshIds::default();
    assert_eq!(used.smallest_unused(), 0);
    assert_eq!(used.max_used(), -1);
    used.insert(1);
    assert_eq!(used.smallest_unused(), 0);
    assert!(used.contains(1));
    assert!(!used.contains(0));
    used.insert(0);
    used.insert(3);
    assert_eq!(used.smallest_unused(), 2);
    assert_eq!(used.max_used(), 3);
    used.insert(2);
    assert_eq!(used.smallest_unused(), 4);
    used.insert(2);
    assert_eq!(used.smallest_unused(), 4);
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

#[test]
fn spans_nested_to_max_depth_stay_valid() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    for _ in 0..MAX_DEPTH {
        tc.start_span(1);
    }
    assert_eq!(tc.status(), None);
}

#[test]
fn spans_nested_past_max_depth_conclude_invalid() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    for _ in 0..=MAX_DEPTH {
        tc.start_span(1);
    }
    assert_eq!(tc.status(), Some(Status::Invalid));
}

mod float_categories {
    use super::*;

    const ENDPOINT: usize = 0;
    const NEAR_ZERO: usize = 1;
    const SUBNORMAL: usize = 2;
    const NEAR_ONE: usize = 3;
    const INTEGER: usize = 4;
    const HALF_INTEGER: usize = 5;
    const NEAR_MAX_FOR_ADD: usize = 6;
    const NEAR_MAX_FOR_MUL: usize = 7;
    const NEAR_SQRT_MIN_POSITIVE: usize = 8;
    const NAN: usize = 9;
    const INFINITY: usize = 10;
    const MAX_MAGNITUDE: usize = 11;
    const MAX_EXACT_INTEGER: usize = 12;
    const SIGNED_ZERO: usize = 13;
    const BINADE_EDGE: usize = 14;
    const NON_DYADIC: usize = 15;
    const DEFAULT: usize = 16;
    const N_CATEGORIES: usize = 17;

    const F32_MAX: f64 = f32::MAX as f64;
    const F32_MIN_POSITIVE: f64 = f32::MIN_POSITIVE as f64;
    const TWO_53: f64 = 9007199254740992.0;
    const TWO_24: f64 = 16777216.0;

    fn choice(min: f64, max: f64) -> FloatChoice {
        FloatChoice {
            min_value: min,
            max_value: max,
            allow_nan: false,
            allow_infinity: false,
            smallest_nonzero_magnitude: 5e-324,
        }
    }

    fn unbounded() -> FloatChoice {
        FloatChoice {
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            allow_nan: true,
            allow_infinity: true,
            smallest_nonzero_magnitude: 5e-324,
        }
    }

    fn half_bounded(min: f64, max: f64) -> FloatChoice {
        FloatChoice {
            allow_infinity: true,
            ..choice(min, max)
        }
    }

    fn with_snm(fc: FloatChoice, snm: f64) -> FloatChoice {
        FloatChoice {
            smallest_nonzero_magnitude: snm,
            ..fc
        }
    }

    /// All the mass on one category, so a draw exercises exactly that sampler
    /// or, when it has nothing valid to offer, the fall-through to the default
    /// draw.
    fn only(category: usize) -> FloatGenerationParameters {
        let mut weights = [0.0; N_CATEGORIES];
        weights[category] = 1.0;
        FloatGenerationParameters::from_weights(weights)
    }

    fn draws_at(
        fc: &FloatChoice,
        width: FloatWidth,
        params: FloatGenerationParameters,
        seed: u64,
        n: usize,
    ) -> Vec<f64> {
        let mut rng = EngineRng::seeded(seed);
        (0..n)
            .map(|_| {
                let v = biased_float_sample(fc, width, &mut rng, params).unwrap();
                assert!(fc.validate(v), "{v:?} is invalid for {fc:?}");
                v
            })
            .collect()
    }

    fn draws(fc: &FloatChoice, params: FloatGenerationParameters, seed: u64, n: usize) -> Vec<f64> {
        draws_at(fc, FloatWidth::F64, params, seed, n)
    }

    fn category_draws_at(fc: &FloatChoice, width: FloatWidth, category: usize) -> Vec<f64> {
        draws_at(fc, width, only(category), 1000 + category as u64, 2000)
    }

    fn category_draws(fc: &FloatChoice, category: usize) -> Vec<f64> {
        category_draws_at(fc, FloatWidth::F64, category)
    }

    /// Whether `v` is exactly representable as an `f32`.
    fn is_f32(v: f64) -> bool {
        f64::from(v as f32) == v
    }

    fn seen(vs: &[f64], target: f64) -> bool {
        vs.iter().any(|v| v.to_bits() == target.to_bits())
    }

    fn assert_exactly_set(vs: &[f64], set: &[f64]) {
        for v in vs {
            assert!(
                set.iter().any(|s| s.to_bits() == v.to_bits()),
                "{v:?} is not one of {set:?}"
            );
        }
        for s in set {
            assert!(seen(vs, *s), "{s:?} never drawn");
        }
    }

    fn in_band(v: f64, (lo, hi): (f64, f64)) -> bool {
        v.abs() >= lo && v.abs() <= hi
    }

    fn all_choices() -> Vec<FloatChoice> {
        vec![
            unbounded(),
            choice(f64::NEG_INFINITY, f64::INFINITY),
            choice(-f64::MAX, f64::MAX),
            choice(0.0, 1.0),
            choice(-1.0, 1.0),
            choice(100.0, 200.0),
            choice(-200.0, -100.0),
            choice(0.05, 10.0),
            choice(0.2, 0.8),
            choice(0.75, 1.25),
            choice(1.0, 4.0),
            with_snm(choice(-1.0, 1.0), 1e-3),
            with_snm(choice(-100.0, 100.0), 10.0),
            choice(f64::from(f32::MIN), F32_MAX),
            half_bounded(f64::NEG_INFINITY, 0.0),
            half_bounded(1.0, f64::INFINITY),
            half_bounded(f64::NEG_INFINITY, -1e300),
            choice(-0.0, 0.0),
            choice(-0.0, -0.0),
            choice(3.5, 3.5),
            choice(1e300, f64::MAX),
            choice(5e-324, 5e-324),
            choice(float_pow2(-1022) / 1e12, float_pow2(-1022) / 1e3),
        ]
    }

    #[test]
    fn every_category_stays_valid_on_every_range() {
        for fc in all_choices() {
            for width in [FloatWidth::F64, FloatWidth::F32] {
                for category in 0..N_CATEGORIES {
                    draws_at(&fc, width, only(category), 7 + category as u64, 300);
                }
                draws_at(&fc, width, FloatGenerationParameters::default(), 99, 300);
                let mut rng = EngineRng::seeded(5);
                for _ in 0..50 {
                    let params = FloatGenerationParameters::draw(&mut rng).unwrap();
                    draws_at(&fc, width, params, 6, 20);
                }
            }
        }
    }

    #[test]
    fn singleton_range_only_ever_draws_its_value() {
        let fc = choice(3.5, 3.5);
        for category in 0..N_CATEGORIES {
            for v in category_draws(&fc, category) {
                assert_eq!(v, 3.5);
            }
        }
    }

    #[test]
    fn endpoint_category_draws_edges_and_neighbours() {
        assert_exactly_set(
            &category_draws(&unbounded(), ENDPOINT),
            &[f64::INFINITY, f64::NEG_INFINITY, f64::MAX, -f64::MAX],
        );
        assert_exactly_set(
            &category_draws(&choice(0.0, 1.0), ENDPOINT),
            &[0.0, 1.0, 5e-324, 1.0f64.next_down()],
        );
        assert_exactly_set(
            &category_draws_at(&choice(0.0, 1.0), FloatWidth::F32, ENDPOINT),
            &[
                0.0,
                1.0,
                f64::from(f32::from_bits(1)),
                f64::from(1.0f32.next_down()),
            ],
        );
        assert_exactly_set(
            &category_draws_at(&unbounded(), FloatWidth::F32, ENDPOINT),
            &[f64::INFINITY, f64::NEG_INFINITY, F32_MAX, -F32_MAX],
        );
        assert_exactly_set(
            &category_draws(&choice(100.0, 200.0), ENDPOINT),
            &[
                100.0,
                200.0,
                100.0f64.next_up(),
                101.0,
                199.0,
                200.0f64.next_down(),
            ],
        );
    }

    #[test]
    fn near_zero_category_is_log_uniform_below_a_tenth() {
        let vs = category_draws(&unbounded(), NEAR_ZERO);
        let mut tiny = 0;
        let (mut positive, mut negative) = (0, 0);
        for &v in &vs {
            assert!(v != 0.0 && v.abs() < 0.1 && !v.is_subnormal(), "{v:e}");
            if v.abs() < 1e-100 {
                tiny += 1;
            }
            if v > 0.0 {
                positive += 1;
            } else {
                negative += 1;
            }
        }
        let tiny_fraction = tiny as f64 / vs.len() as f64;
        assert!(
            tiny_fraction > 0.2,
            "only {tiny_fraction} of near-zero draws were below 1e-100"
        );
        assert!(positive > 500 && negative > 500, "{positive} / {negative}");

        for v in category_draws(&choice(0.05, 10.0), NEAR_ZERO) {
            assert!((0.05..0.1).contains(&v), "{v}");
        }

        let vs = category_draws_at(&unbounded(), FloatWidth::F32, NEAR_ZERO);
        assert!(
            vs.iter()
                .all(|&v| v.abs() >= F32_MIN_POSITIVE && v.abs() < 0.1)
        );
        assert!(vs.iter().filter(|&&v| v.abs() < 1e-20).count() > 500);
    }

    #[test]
    fn subnormal_category_matches_the_width() {
        let vs = category_draws(&unbounded(), SUBNORMAL);
        assert!(vs.iter().all(|v| v.is_subnormal()), "{vs:?}");
        assert!(vs.iter().any(|&v| v < 0.0) && vs.iter().any(|&v| v > 0.0));

        let vs = category_draws_at(&unbounded(), FloatWidth::F32, SUBNORMAL);
        for &v in &vs {
            assert!(
                v.abs() >= f64::from(f32::from_bits(1)) && v.abs() < F32_MIN_POSITIVE,
                "{v:e}"
            );
            assert!(!v.is_subnormal(), "{v:e}");
        }
        assert!(vs.iter().any(|&v| v < 0.0) && vs.iter().any(|&v| v > 0.0));
    }

    #[test]
    fn near_one_category_reaches_ulp_neighbours_and_exact_values_on_both_signs() {
        let vs = category_draws(&unbounded(), NEAR_ONE);
        let mut sides = [[0; 2]; 2];
        for &v in &vs {
            let mag = v.abs();
            assert!(mag > 0.5 && mag < 1.5, "{v}");
            if mag != 1.0 {
                sides[usize::from(v < 0.0)][usize::from(mag > 1.0)] += 1;
            }
        }
        for side in sides.iter().flatten() {
            assert!(*side > 250, "{sides:?}");
        }
        for sign in [1.0, -1.0] {
            assert!(seen(&vs, sign));
            assert!(seen(&vs, sign * 1.0f64.next_up()));
            assert!(seen(&vs, sign * 1.0f64.next_down()));
        }

        for v in category_draws(&choice(0.0, 1.0), NEAR_ONE) {
            assert!(v > 0.5 && v <= 1.0, "{v}");
        }
        for v in category_draws(&choice(1.0, 2.0), NEAR_ONE) {
            assert!((1.0..1.5).contains(&v), "{v}");
        }
        for v in category_draws(&choice(-1.0, -0.75), NEAR_ONE) {
            assert!((-1.0..=-0.75).contains(&v), "{v}");
        }

        let vs = category_draws_at(&unbounded(), FloatWidth::F32, NEAR_ONE);
        for &v in &vs {
            assert!(is_f32(v) && v.abs() > 0.5 && v.abs() < 1.5, "{v}");
        }
        for sign in [1.0, -1.0] {
            assert!(seen(&vs, sign));
            assert!(seen(&vs, sign * f64::from(1.0f32.next_up())));
            assert!(seen(&vs, sign * f64::from(1.0f32.next_down())));
        }
        for v in category_draws_at(&choice(0.0, 1.0), FloatWidth::F32, NEAR_ONE) {
            assert!(is_f32(v) && v > 0.5 && v <= 1.0, "{v}");
        }
    }

    /// The sign is a coin flip, but a sign on which the range admits nothing
    /// for the category yields to the other rather than falling through to
    /// the uniform, so a one-sided range gets the category every time.
    #[test]
    fn sign_yields_to_the_side_the_range_admits() {
        let fc = choice(-f64::MAX, 0.5);
        for v in category_draws(&fc, NEAR_MAX_FOR_ADD) {
            assert!(v <= -float_pow2(127), "{v:e}");
        }
        for v in category_draws(&fc, NEAR_MAX_FOR_MUL) {
            assert!(v <= -float_pow2(63), "{v:e}");
        }
        for v in category_draws(&choice(-0.5, 1e300), NEAR_MAX_FOR_MUL) {
            assert!(v >= float_pow2(63), "{v:e}");
        }
        for v in category_draws(&choice(-2.0, 0.25), NEAR_ONE) {
            assert!(v > -1.5 && v < -0.5, "{v}");
        }
    }

    /// Integers and half-integers spread over the binades of magnitude, so
    /// small values (1, 2, 0.5, 2.5, …) come up as often as the huge ones a
    /// uniform draw over `±2^53` would produce almost exclusively.
    #[test]
    fn integer_categories_are_log_uniform_over_their_binades() {
        let vs = category_draws(&unbounded(), INTEGER);
        let (mut positive, mut negative, mut small) = (0, 0, 0);
        for &v in &vs {
            assert!(v == v.trunc() && v != 0.0 && v.abs() <= TWO_53, "{v}");
            if v > 0.0 {
                positive += 1;
            } else {
                negative += 1;
            }
            if v.abs() < 1000.0 {
                small += 1;
            }
        }
        assert!(positive > 500 && negative > 500, "{positive} / {negative}");
        assert!(
            small > 250,
            "only {small} of {} integers were below 1000",
            vs.len()
        );
        assert!(seen(&vs, 1.0) && seen(&vs, -1.0));
        assert!(seen(&vs, TWO_53) || seen(&vs, -TWO_53));
        assert_exactly_set(
            &category_draws(&choice(0.0, 10.0), INTEGER),
            &(1..=10).map(f64::from).collect::<Vec<_>>(),
        );
        assert_exactly_set(
            &category_draws(&choice(-3.0, 0.5), INTEGER),
            &[-1.0, -2.0, -3.0],
        );

        let halves = category_draws(&unbounded(), HALF_INTEGER);
        let mut small = 0;
        for &v in &halves {
            assert!(
                v.fract().abs() == 0.5 && v.abs() < 4503599627370496.0,
                "{v}"
            );
            if v.abs() < 1000.0 {
                small += 1;
            }
        }
        assert!(
            small > 250,
            "only {small} of {} half-integers were below 1000",
            halves.len()
        );
        assert!(seen(&halves, 0.5) && seen(&halves, -0.5));
        assert!(seen(&halves, 1.5) || seen(&halves, -1.5));
        assert_exactly_set(
            &category_draws(&choice(0.0, 3.0), HALF_INTEGER),
            &[0.5, 1.5, 2.5],
        );
        assert_exactly_set(
            &category_draws(&choice(-2.0, 0.6), HALF_INTEGER),
            &[0.5, -0.5, -1.5],
        );

        let vs = category_draws_at(&unbounded(), FloatWidth::F32, INTEGER);
        assert!(
            vs.iter()
                .all(|&v| v == v.trunc() && v != 0.0 && v.abs() <= TWO_24)
        );
        assert!(seen(&vs, TWO_24) || seen(&vs, -TWO_24));
        assert!(seen(&vs, 1.0) || seen(&vs, -1.0));
        let halves = category_draws_at(&unbounded(), FloatWidth::F32, HALF_INTEGER);
        assert!(
            halves
                .iter()
                .all(|&v| v.fract().abs() == 0.5 && v.abs() < TWO_24 / 2.0)
        );
        assert!(halves.iter().all(|&v| is_f32(v)));
    }

    #[test]
    fn log_uniform_integer_weights_binades_evenly() {
        let mut rng = EngineRng::seeded(78);
        let n = 44_000;
        let mut counts = [0u32; 11];
        for _ in 0..n {
            let v = log_uniform_integer(0, 1023, &mut rng);
            counts[(64 - v.leading_zeros()) as usize] += 1;
        }
        for (b, &c) in counts.iter().enumerate() {
            let share = c as f64 / n as f64;
            assert!((share - 1.0 / 11.0).abs() < 0.015, "bucket {b}: {share}");
        }
        for _ in 0..1000 {
            let v = log_uniform_integer(6, 9, &mut rng);
            assert!((6..=9).contains(&v), "{v}");
        }
        assert_eq!(log_uniform_integer(1 << 53, 1 << 53, &mut rng), 1 << 53);
    }

    #[test]
    fn overflow_and_underflow_bands_make_pairs_misbehave() {
        let adds = category_draws(&unbounded(), NEAR_MAX_FOR_ADD);
        for &v in &adds {
            assert!(in_band(v, (float_pow2(1023), f64::MAX)), "{v:e}");
            assert!((v + v).is_infinite());
        }
        assert!(adds.iter().any(|&v| v < 0.0) && adds.iter().any(|&v| v > 0.0));
        let adds = category_draws_at(&unbounded(), FloatWidth::F32, NEAR_MAX_FOR_ADD);
        for &v in &adds {
            assert!(in_band(v, (float_pow2(127), f64::from(f32::MAX))), "{v:e}");
            assert!((v as f32 + v as f32).is_infinite());
            assert!((v + v).is_finite());
        }

        let muls = category_draws(&unbounded(), NEAR_MAX_FOR_MUL);
        assert!(
            muls.iter()
                .all(|&v| in_band(v, (float_pow2(511), float_below(float_pow2(513)))))
        );
        let overflowing = muls
            .windows(2)
            .filter(|w| (w[0] * w[1]).is_infinite())
            .count();
        assert!(
            overflowing > 300 && overflowing < muls.len() - 300,
            "{overflowing}"
        );
        let muls = category_draws_at(&unbounded(), FloatWidth::F32, NEAR_MAX_FOR_MUL);
        assert!(
            muls.iter()
                .all(|&v| in_band(v, (float_pow2(63), float_below(float_pow2(65)))))
        );
        let overflowing = muls
            .windows(2)
            .filter(|w| (w[0] as f32 * w[1] as f32).is_infinite())
            .count();
        assert!(
            overflowing > 300 && overflowing < muls.len() - 300,
            "{overflowing}"
        );

        let tinies = category_draws(&unbounded(), NEAR_SQRT_MIN_POSITIVE);
        assert!(
            tinies
                .iter()
                .all(|&v| in_band(v, (float_pow2(-513), float_below(float_pow2(-510)))))
        );
        let underflowing = tinies
            .windows(2)
            .filter(|w| (w[0] * w[1]).is_subnormal())
            .count();
        assert!(
            underflowing > 300 && underflowing < tinies.len() - 300,
            "{underflowing}"
        );
        let tinies = category_draws_at(&unbounded(), FloatWidth::F32, NEAR_SQRT_MIN_POSITIVE);
        assert!(
            tinies
                .iter()
                .all(|&v| in_band(v, (float_pow2(-65), float_below(float_pow2(-62)))))
        );
        let underflowing = tinies
            .windows(2)
            .filter(|w| (w[0] as f32 * w[1] as f32).is_subnormal())
            .count();
        assert!(
            underflowing > 300 && underflowing < tinies.len() - 300,
            "{underflowing}"
        );
    }

    #[test]
    fn nan_category_varies_sign_and_payload() {
        let vs = category_draws(&unbounded(), NAN);
        let (mut negative, mut canonical, mut signalling, mut other_payload) = (0, 0, 0, 0);
        for &v in &vs {
            assert!(v.is_nan());
            let bits = v.to_bits();
            if bits >> 63 == 1 {
                negative += 1;
            }
            let payload = bits & ((1u64 << 52) - 1);
            if payload == 1 << 51 {
                canonical += 1;
            } else if payload >> 51 == 0 {
                signalling += 1;
            } else {
                other_payload += 1;
            }
        }
        assert!(negative > 500, "{negative}");
        assert!(canonical > 500, "{canonical}");
        assert!(signalling > 200, "{signalling}");
        assert!(other_payload > 200, "{other_payload}");
        assert!(
            !category_draws(&choice(0.0, 1.0), NAN)
                .iter()
                .any(|v| v.is_nan())
        );
    }

    #[test]
    fn point_categories_draw_exactly_their_landmarks() {
        let fc = unbounded();
        assert_exactly_set(
            &category_draws(&fc, INFINITY),
            &[f64::INFINITY, f64::NEG_INFINITY],
        );
        assert_exactly_set(&category_draws(&fc, MAX_MAGNITUDE), &[f64::MAX, -f64::MAX]);
        assert_exactly_set(
            &category_draws_at(&fc, FloatWidth::F32, MAX_MAGNITUDE),
            &[F32_MAX, -F32_MAX],
        );
        assert_exactly_set(&category_draws(&fc, MAX_EXACT_INTEGER), &[TWO_53, -TWO_53]);
        assert_exactly_set(
            &category_draws_at(&fc, FloatWidth::F32, MAX_EXACT_INTEGER),
            &[TWO_24, -TWO_24],
        );
        assert_exactly_set(&category_draws(&fc, SIGNED_ZERO), &[0.0, -0.0]);

        assert_exactly_set(
            &category_draws(&half_bounded(1.0, f64::INFINITY), INFINITY),
            &[f64::INFINITY],
        );
        assert_exactly_set(
            &category_draws(&half_bounded(f64::NEG_INFINITY, 0.0), INFINITY),
            &[f64::NEG_INFINITY],
        );
        assert_exactly_set(
            &category_draws_at(
                &choice(f64::from(f32::MIN), F32_MAX),
                FloatWidth::F32,
                MAX_MAGNITUDE,
            ),
            &[F32_MAX, -F32_MAX],
        );
        assert_exactly_set(
            &category_draws_at(&choice(0.0, 1e8), FloatWidth::F32, MAX_EXACT_INTEGER),
            &[TWO_24],
        );
        assert!(
            !category_draws(&choice(0.0, 1e8), MAX_EXACT_INTEGER)
                .iter()
                .all(|&v| v == TWO_24),
            "an f64 draw offered f32's landmark"
        );
        assert_exactly_set(&category_draws(&choice(0.0, 1.0), SIGNED_ZERO), &[0.0]);
        assert_exactly_set(&category_draws(&choice(-1.0, -0.0), SIGNED_ZERO), &[-0.0]);
    }

    #[test]
    fn binade_edge_category_draws_powers_of_two_and_predecessors() {
        let vs = category_draws(&unbounded(), BINADE_EDGE);
        let (mut powers, mut predecessors, mut negative) = (0, 0, 0);
        let mut exponents = std::collections::HashSet::new();
        for &v in &vs {
            let mantissa = v.to_bits() & FLOAT_MANTISSA_MASK;
            if mantissa == 0 {
                assert!(v.is_normal(), "{v:e}");
                powers += 1;
            } else {
                assert_eq!(mantissa, FLOAT_MANTISSA_MASK, "{v:e}");
                predecessors += 1;
            }
            if v < 0.0 {
                negative += 1;
            }
            exponents.insert((v.to_bits() >> 52) & 0x7FF);
        }
        assert!(
            powers > 500 && predecessors > 500,
            "{powers} / {predecessors}"
        );
        assert!(negative > 500, "{negative}");
        assert!(exponents.len() > 500, "{}", exponents.len());

        assert_exactly_set(
            &category_draws(&choice(1.0, 4.0), BINADE_EDGE),
            &[1.0, 2.0, 4.0, 2.0f64.next_down(), 4.0f64.next_down()],
        );
        assert_exactly_set(
            &category_draws(&choice(-8.0, -3.0), BINADE_EDGE),
            &[-4.0, -8.0, -(4.0f64.next_down()), -(8.0f64.next_down())],
        );
        assert_exactly_set(&category_draws(&choice(2.0, 2.0), BINADE_EDGE), &[2.0]);
        for v in category_draws(&choice(0.0, 1e-310), BINADE_EDGE) {
            assert!(v == 0.0 || v.is_subnormal(), "{v:e}");
        }

        let vs = category_draws_at(&unbounded(), FloatWidth::F32, BINADE_EDGE);
        let mut exponents = std::collections::HashSet::new();
        let (mut powers, mut predecessors) = (0, 0);
        for &v in &vs {
            assert!(is_f32(v), "{v:e}");
            let bits = (v as f32).to_bits();
            let mantissa = bits & ((1u32 << 23) - 1);
            if mantissa == 0 {
                assert!((v as f32).is_normal(), "{v:e}");
                powers += 1;
            } else {
                assert_eq!(mantissa, (1u32 << 23) - 1, "{v:e}");
                predecessors += 1;
            }
            exponents.insert((bits >> 23) & 0xFF);
        }
        assert!(
            powers > 500 && predecessors > 500,
            "{powers} / {predecessors}"
        );
        assert!(exponents.len() > 100, "{}", exponents.len());
        assert_exactly_set(
            &category_draws_at(&choice(1.0, 4.0), FloatWidth::F32, BINADE_EDGE),
            &[
                1.0,
                2.0,
                4.0,
                f64::from(2.0f32.next_down()),
                f64::from(4.0f32.next_down()),
            ],
        );
    }

    #[test]
    fn non_dyadic_category_draws_small_non_integers_or_fits_the_range() {
        let vs = category_draws(&unbounded(), NON_DYADIC);
        let mut long_fraction = 0;
        for &v in &vs {
            assert!(v.is_finite() && v.abs() <= 101.0 && v != v.trunc(), "{v}");
            if (v * 100.0).fract() != 0.0 {
                long_fraction += 1;
            }
        }
        assert!(long_fraction > 500, "{long_fraction}");
        assert!(vs.iter().any(|&v| v < 0.0) && vs.iter().any(|&v| v > 0.0));
        assert!(
            vs.iter().filter(|v| v.abs().to_string().len() <= 5).count() > 50,
            "no short decimals"
        );

        for v in category_draws(&choice(0.0, 1.0), NON_DYADIC) {
            assert!(v > 0.0 && v < 1.0 && v != v.trunc(), "{v}");
        }
        for v in category_draws(&choice(-1.0, -0.5), NON_DYADIC) {
            assert!((-1.0..=-0.5).contains(&v), "{v}");
        }
        let fitted = category_draws(&choice(100.0, 200.0), NON_DYADIC);
        assert!(fitted.iter().all(|&v| (100.0..=200.0).contains(&v)));
        assert!(fitted.iter().filter(|&&v| v > 101.0).count() > 1000);
    }

    /// The default is a coin flip between the continuous uniform and the
    /// log-uniform, so a bounded range shows both an ordinary middle and a
    /// spread over its small scales, reaching all the way down to the deep
    /// binades far below the top bound.
    #[test]
    fn default_draw_is_half_uniform_half_log_uniform() {
        let vs = category_draws(&choice(0.0, 10.0), DEFAULT);
        let (mut tiny, mut deep, mut upper_half) = (0, 0, 0);
        for &v in &vs {
            assert!((0.0..=10.0).contains(&v), "{v:e}");
            if v < 1e-6 {
                tiny += 1;
            }
            if v != 0.0 && v < 1e-100 {
                deep += 1;
            }
            if v > 5.0 {
                upper_half += 1;
            }
        }
        let tiny_fraction = tiny as f64 / vs.len() as f64;
        assert!(
            (0.35..0.6).contains(&tiny_fraction),
            "{tiny_fraction} of default draws on [0, 10] were below 1e-6; expected \
             nearly all of the log-uniform half"
        );
        assert!(
            deep > 200,
            "{deep} of {} default draws on [0, 10] were below 1e-100; the log-uniform \
             half must reach the deep binades under a finite bound",
            vs.len()
        );
        let upper_fraction = upper_half as f64 / vs.len() as f64;
        assert!(
            (0.15..0.35).contains(&upper_fraction),
            "{upper_fraction} of default draws on [0, 10] were above 5; expected \
             about half of the uniform half"
        );

        let vs = category_draws(&choice(1e-8, 2e4), DEFAULT);
        let below_one = vs.iter().filter(|&&v| v < 1.0).count() as f64 / vs.len() as f64;
        assert!((0.25..0.5).contains(&below_one), "{below_one}");

        let vs = category_draws(&unbounded(), DEFAULT);
        let finite: Vec<f64> = vs.iter().copied().filter(|v| v.is_finite()).collect();
        assert!(finite.len() > 1900, "{}", finite.len());
        let (mut positive, mut negative, mut tiny) = (0, 0, 0);
        for &v in &finite {
            if v > 0.0 {
                positive += 1;
            } else if v < 0.0 {
                negative += 1;
            }
            if v != 0.0 && v.abs() < 1e-200 {
                tiny += 1;
            }
        }
        assert!(positive > 500 && negative > 500, "{positive} / {negative}");
        assert!(tiny > 120, "{tiny}");
        assert!(finite.iter().any(|&v| v.abs() > 1e100));

        let vs = category_draws(&choice(-1.0, 1.0), DEFAULT);
        assert!(vs.iter().all(|&v| v.abs() <= 1.0));
        let below = vs.iter().filter(|&&v| v != 0.0 && v.abs() < 1e-10).count();
        assert!(below > 150, "{below}");

        let wide = category_draws(&choice(-f64::MAX, f64::MAX), DEFAULT);
        assert!(wide.iter().all(|v| v.is_finite()));
        let clamped = category_draws(&with_snm(choice(-1.0, 1.0), 0.5), DEFAULT);
        assert!(clamped.iter().all(|&v| v == 0.0 || v.abs() >= 0.5));
    }

    /// A category the range rules out sends its mass to the default draw, not
    /// to the next category in weight order. On `[0.3, 0.45]`, inside one
    /// binade, both halves of the default are a plain uniform, so the
    /// fall-through shows as the right mean and no collapse onto a few values.
    #[test]
    fn unavailable_category_falls_through_to_the_default() {
        let fc = choice(0.3, 0.45);
        for category in [
            NEAR_ZERO,
            SUBNORMAL,
            NEAR_ONE,
            INTEGER,
            HALF_INTEGER,
            NEAR_MAX_FOR_ADD,
            NEAR_MAX_FOR_MUL,
            NEAR_SQRT_MIN_POSITIVE,
            NAN,
            INFINITY,
            MAX_MAGNITUDE,
            MAX_EXACT_INTEGER,
            SIGNED_ZERO,
            BINADE_EDGE,
        ] {
            let vs = category_draws(&fc, category);
            let mean = vs.iter().sum::<f64>() / vs.len() as f64;
            assert!(
                (mean - 0.375).abs() < 0.01,
                "category {category}: mean {mean}"
            );
            let distinct: std::collections::HashSet<u64> = vs.iter().map(|v| v.to_bits()).collect();
            assert!(
                distinct.len() > 1900,
                "category {category}: {}",
                distinct.len()
            );
        }
    }

    #[test]
    fn category_weights_control_the_mix() {
        let fc = choice(0.0, 1.0);
        let mut weights = [0.0; N_CATEGORIES];
        weights[INTEGER] = 0.5;
        weights[DEFAULT] = 0.5;
        let half_integers = draws(
            &fc,
            FloatGenerationParameters::from_weights(weights),
            3,
            20_000,
        );
        let integral = half_integers.iter().filter(|&&v| v == v.trunc()).count() as f64
            / half_integers.len() as f64;
        assert!((integral - 0.5).abs() < 0.03, "{integral}");

        let mut weights = [0.0; N_CATEGORIES];
        weights[NAN] = 0.25;
        weights[SIGNED_ZERO] = 0.25;
        weights[DEFAULT] = 0.5;
        let vs = draws(
            &unbounded(),
            FloatGenerationParameters::from_weights(weights),
            4,
            20_000,
        );
        let nan = vs.iter().filter(|v| v.is_nan()).count() as f64 / vs.len() as f64;
        let zero = vs.iter().filter(|&&v| v == 0.0).count() as f64 / vs.len() as f64;
        assert!((nan - 0.25).abs() < 0.03, "{nan}");
        assert!((zero - 0.25).abs() < 0.03, "{zero}");
    }

    #[test]
    fn default_weights_surface_every_category_on_the_unbounded_range() {
        let vs = draws(
            &unbounded(),
            FloatGenerationParameters::default(),
            11,
            20_000,
        );
        let rate = |pred: &dyn Fn(f64) -> bool| {
            vs.iter().filter(|&&v| pred(v)).count() as f64 / vs.len() as f64
        };
        let checks: [(&str, &dyn Fn(f64) -> bool); 9] = [
            ("nan", &|v| v.is_nan()),
            ("infinite", &|v| v.is_infinite()),
            ("zero", &|v| v == 0.0),
            ("subnormal", &|v| v.is_subnormal()),
            ("near ±1", &|v| {
                v.abs() != 1.0 && (v.abs() - 1.0).abs() < 1e-9
            }),
            ("half integer", &|v| v.is_finite() && v.fract().abs() == 0.5),
            ("top binade", &|v| {
                v.is_finite() && v.abs() >= float_pow2(1023)
            }),
            ("max exact integer", &|v| {
                v.abs() == TWO_53 || v.abs() == TWO_24
            }),
            ("tiny", &|v| v != 0.0 && v.abs() < 1e-200),
        ];
        for (name, pred) in checks {
            let r = rate(pred);
            assert!(r > 0.01, "{name} rate {r:.4}");
        }
    }

    #[test]
    fn log_uniform_magnitude_weights_binades_evenly() {
        let mut rng = EngineRng::seeded(77);
        let (lo, hi) = (float_pow2(-10), float_pow2(10));
        let mut counts = [0u32; 21];
        let n = 42_000;
        for _ in 0..n {
            let v = log_uniform_magnitude(lo, hi, &mut rng);
            assert!((lo..=hi).contains(&v), "{v}");
            counts[(((v.to_bits() >> 52) & 0x7FF) as i64 - 1013) as usize] += 1;
        }
        for (i, &c) in counts.iter().enumerate() {
            let share = c as f64 / n as f64;
            assert!((share - 1.0 / 21.0).abs() < 0.015, "binade {i}: {share}");
        }
        for _ in 0..1000 {
            let v = log_uniform_magnitude(1.5, 1.75, &mut rng);
            assert!((1.5..=1.75).contains(&v), "{v}");
            let s = log_uniform_magnitude(5e-324, 1e-320, &mut rng);
            assert!(s.is_subnormal() && s <= 1e-320, "{s:e}");
        }
    }

    #[test]
    fn float_below_is_the_finite_predecessor() {
        assert_eq!(float_below(1.0), 1.0f64.next_down());
        assert_eq!(float_below(0.1), 0.1f64.next_down());
        assert_eq!(float_below(float_pow2(513)), float_pow2(513).next_down());
        assert!(float_below(f64::MIN_POSITIVE).is_subnormal());
    }

    type Sampler = fn(&FloatChoice, FloatWidth, &mut EngineRng) -> Option<f64>;

    fn direct_draws(sample: Sampler, fc: &FloatChoice, width: FloatWidth) -> Vec<f64> {
        let mut rng = EngineRng::seeded(42);
        (0..500)
            .map(|_| sample(fc, width, &mut rng).unwrap())
            .collect()
    }

    #[test]
    fn float_pow2_is_the_exact_power_of_two() {
        assert_eq!(float_pow2(0), 1.0);
        assert_eq!(float_pow2(10), 1024.0);
        assert_eq!(float_pow2(-3), 0.125);
        assert_eq!(float_pow2(1023), 2.0f64.powi(1023));
        assert_eq!(float_pow2(-1022), f64::MIN_POSITIVE);
        assert_eq!(float_pow2(-1023), f64::MIN_POSITIVE / 2.0);
        assert_eq!(float_pow2(-1074), 5e-324);
        assert_eq!(float_pow2(-126), F32_MIN_POSITIVE);
    }

    #[test]
    fn float_width_layout_gives_the_exponent_bias_and_mantissa_bits() {
        assert_eq!(FloatWidth::F64.layout(), (1023, 52));
        assert_eq!(FloatWidth::F32.layout(), (127, 23));
        let (e, m) = FloatWidth::F64.layout();
        assert_eq!(float_pow2(1 - e), f64::MIN_POSITIVE);
        assert_eq!(float_pow2(m + 1), TWO_53);
        let (e, m) = FloatWidth::F32.layout();
        assert_eq!(float_pow2(1 - e), F32_MIN_POSITIVE);
        assert_eq!(float_pow2(m + 1), TWO_24);
    }

    #[test]
    fn float_width_max_is_the_widths_largest_finite() {
        assert_eq!(FloatWidth::F64.max(), f64::MAX);
        assert_eq!(FloatWidth::F32.max(), F32_MAX);
    }

    #[test]
    fn float_width_bit_range_rounds_f32_bounds_inward() {
        assert_eq!(
            FloatWidth::F64.bit_range(1.0, 2.0),
            (1.0f64.to_bits(), 2.0f64.to_bits())
        );
        assert_eq!(
            FloatWidth::F32.bit_range(1.0, 2.0),
            (u64::from(1.0f32.to_bits()), u64::from(2.0f32.to_bits()))
        );
        let (lo, hi) = FloatWidth::F32.bit_range(1.0f64.next_up(), 2.0f64.next_down());
        assert_eq!(lo, u64::from(1.0f32.next_up().to_bits()));
        assert_eq!(hi, u64::from(2.0f32.next_down().to_bits()));
        let (lo, hi) = FloatWidth::F32.bit_range(1.0f64.next_up(), 1.0f64.next_up().next_up());
        assert!(lo > hi, "{lo} <= {hi}");
    }

    #[test]
    fn float_width_to_float_decodes_the_widths_bit_pattern() {
        assert_eq!(FloatWidth::F64.to_float(1.0f64.to_bits()), 1.0);
        assert_eq!(FloatWidth::F64.to_float(1), 5e-324);
        assert_eq!(FloatWidth::F32.to_float(u64::from(1.0f32.to_bits())), 1.0);
        assert_eq!(FloatWidth::F32.to_float(1), f64::from(f32::from_bits(1)));
    }

    #[test]
    fn coin_flip_first_uses_whichever_option_has_a_value() {
        let mut rng = EngineRng::seeded(42);
        let positive = |x: i32, _: &mut EngineRng| (x > 0).then_some(x);
        let both: Vec<i32> = (0..200)
            .map(|_| coin_flip_first([1, 2], &mut rng, positive).unwrap())
            .collect();
        assert!(both.contains(&1) && both.contains(&2));
        for _ in 0..50 {
            assert_eq!(coin_flip_first([0, 2], &mut rng, positive), Some(2));
            assert_eq!(coin_flip_first([1, 0], &mut rng, positive), Some(1));
            assert_eq!(coin_flip_first([0, 0], &mut rng, positive), None);
        }
    }

    #[test]
    fn signed_magnitude_sample_applies_a_sign_the_range_admits() {
        let smallest = |lo: f64, _: f64, _: &mut EngineRng| Some(lo);
        let mut rng = EngineRng::seeded(42);
        let vs: Vec<f64> = (0..200)
            .map(|_| signed_magnitude_sample(&unbounded(), &mut rng, smallest).unwrap())
            .collect();
        assert_exactly_set(&vs, &[5e-324, -5e-324]);
        for _ in 0..50 {
            assert_eq!(
                signed_magnitude_sample(&choice(100.0, 200.0), &mut rng, smallest),
                Some(100.0)
            );
            assert_eq!(
                signed_magnitude_sample(&choice(-200.0, -100.0), &mut rng, smallest),
                Some(-100.0)
            );
            assert_eq!(
                signed_magnitude_sample(&with_snm(choice(-1.0, 1.0), 0.25), &mut rng, smallest)
                    .map(f64::abs),
                Some(0.25)
            );
            assert_eq!(
                signed_magnitude_sample(&choice(-0.0, 0.0), &mut rng, smallest),
                None
            );
        }
    }

    #[test]
    fn banded_magnitude_sample_stays_inside_the_band_and_the_range() {
        let mut rng = EngineRng::seeded(42);
        let band = (1.0, 8.0);
        let vs: Vec<f64> = (0..500)
            .map(|_| banded_magnitude_sample(&unbounded(), band, &mut rng).unwrap())
            .collect();
        assert!(vs.iter().all(|&v| in_band(v, band)), "{vs:?}");
        assert!(vs.iter().any(|&v| v < 0.0) && vs.iter().any(|&v| v > 0.0));
        for _ in 0..100 {
            let v = banded_magnitude_sample(&choice(0.0, 2.0), band, &mut rng).unwrap();
            assert!((1.0..=2.0).contains(&v), "{v}");
            assert_eq!(
                banded_magnitude_sample(&choice(0.0, 0.5), band, &mut rng),
                None
            );
        }
    }

    #[test]
    fn pick_valid_draws_only_valid_candidates_without_double_counting() {
        let fc = choice(0.0, 1.0);
        let mut rng = EngineRng::seeded(42);
        let vs: Vec<f64> = (0..2000)
            .map(|_| pick_valid(&fc, [-1.0, 0.5, 0.5, 2.0, 1.0], &mut rng).unwrap())
            .collect();
        assert_exactly_set(&vs, &[0.5, 1.0]);
        let halves = vs.iter().filter(|&&v| v == 0.5).count();
        assert!((900..1100).contains(&halves), "{halves}");
        assert_eq!(pick_valid(&fc, [-1.0, 2.0], &mut rng), None);
    }

    #[test]
    fn float_endpoint_sample_returns_an_edge_or_its_neighbour() {
        assert_exactly_set(
            &direct_draws(
                float_endpoint_sample,
                &choice(100.0, 200.0),
                FloatWidth::F64,
            ),
            &[
                100.0,
                200.0,
                100.0f64.next_up(),
                101.0,
                199.0,
                200.0f64.next_down(),
            ],
        );
    }

    #[test]
    fn float_near_zero_sample_is_a_small_normal() {
        for v in direct_draws(float_near_zero_sample, &unbounded(), FloatWidth::F64) {
            assert!(v.is_normal() && v.abs() < 0.1, "{v:e}");
        }
        for v in direct_draws(float_near_zero_sample, &unbounded(), FloatWidth::F32) {
            assert!((F32_MIN_POSITIVE..0.1).contains(&v.abs()), "{v:e}");
        }
    }

    #[test]
    fn float_subnormal_sample_is_below_min_positive() {
        for v in direct_draws(float_subnormal_sample, &unbounded(), FloatWidth::F64) {
            assert!(v != 0.0 && v.abs() < f64::MIN_POSITIVE, "{v:e}");
        }
        for v in direct_draws(float_subnormal_sample, &unbounded(), FloatWidth::F32) {
            assert!(v != 0.0 && v.abs() < F32_MIN_POSITIVE, "{v:e}");
        }
    }

    #[test]
    fn float_near_one_sample_is_within_half_of_one() {
        for v in direct_draws(float_near_one_sample, &unbounded(), FloatWidth::F64) {
            assert!(v.abs() > 0.5 && v.abs() < 1.5, "{v}");
        }
    }

    #[test]
    fn float_integer_sample_is_a_nonzero_exact_integer() {
        for v in direct_draws(float_integer_sample, &unbounded(), FloatWidth::F64) {
            assert!(v == v.trunc() && v != 0.0 && v.abs() <= TWO_53, "{v}");
        }
        for v in direct_draws(float_integer_sample, &unbounded(), FloatWidth::F32) {
            assert!(v == v.trunc() && v != 0.0 && v.abs() <= TWO_24, "{v}");
        }
    }

    #[test]
    fn float_half_integer_sample_ends_in_a_half() {
        for v in direct_draws(float_half_integer_sample, &unbounded(), FloatWidth::F64) {
            assert!(v.fract().abs() == 0.5 && v.abs() < TWO_53 / 2.0, "{v}");
        }
    }

    #[test]
    fn float_near_max_for_add_sample_overflows_when_doubled() {
        for v in direct_draws(float_near_max_for_add_sample, &unbounded(), FloatWidth::F64) {
            assert!(v.is_finite() && v.abs() >= float_pow2(1023), "{v:e}");
            assert!((v + v).is_infinite());
        }
        for v in direct_draws(float_near_max_for_add_sample, &unbounded(), FloatWidth::F32) {
            assert!(
                v.abs() <= F32_MAX && (v as f32 + v as f32).is_infinite(),
                "{v:e}"
            );
        }
    }

    #[test]
    fn float_near_max_for_mul_sample_sits_around_sqrt_max() {
        for v in direct_draws(float_near_max_for_mul_sample, &unbounded(), FloatWidth::F64) {
            assert!(
                (float_pow2(511)..float_pow2(513)).contains(&v.abs()),
                "{v:e}"
            );
        }
        for v in direct_draws(float_near_max_for_mul_sample, &unbounded(), FloatWidth::F32) {
            assert!((float_pow2(63)..float_pow2(65)).contains(&v.abs()), "{v:e}");
        }
    }

    #[test]
    fn float_near_sqrt_min_positive_sample_sits_around_sqrt_min_positive() {
        for v in direct_draws(
            float_near_sqrt_min_positive_sample,
            &unbounded(),
            FloatWidth::F64,
        ) {
            assert!(
                (float_pow2(-513)..float_pow2(-510)).contains(&v.abs()),
                "{v:e}"
            );
        }
        for v in direct_draws(
            float_near_sqrt_min_positive_sample,
            &unbounded(),
            FloatWidth::F32,
        ) {
            assert!(
                (float_pow2(-65)..float_pow2(-62)).contains(&v.abs()),
                "{v:e}"
            );
        }
    }

    #[test]
    fn float_nan_sample_is_nan_only_when_allowed() {
        for v in direct_draws(float_nan_sample, &unbounded(), FloatWidth::F64) {
            assert!(v.is_nan());
        }
        let mut rng = EngineRng::seeded(42);
        assert_eq!(
            float_nan_sample(&choice(0.0, 1.0), FloatWidth::F64, &mut rng),
            None
        );
    }

    #[test]
    fn float_infinity_sample_is_an_admitted_infinity() {
        assert_exactly_set(
            &direct_draws(float_infinity_sample, &unbounded(), FloatWidth::F64),
            &[f64::INFINITY, f64::NEG_INFINITY],
        );
        let mut rng = EngineRng::seeded(42);
        assert_eq!(
            float_infinity_sample(&choice(0.0, 1.0), FloatWidth::F64, &mut rng),
            None
        );
    }

    #[test]
    fn float_max_magnitude_sample_is_plus_or_minus_max_of_the_width() {
        assert_exactly_set(
            &direct_draws(float_max_magnitude_sample, &unbounded(), FloatWidth::F64),
            &[f64::MAX, -f64::MAX],
        );
        assert_exactly_set(
            &direct_draws(float_max_magnitude_sample, &unbounded(), FloatWidth::F32),
            &[F32_MAX, -F32_MAX],
        );
    }

    #[test]
    fn float_max_exact_integer_sample_is_plus_or_minus_two_to_the_mantissa() {
        assert_exactly_set(
            &direct_draws(
                float_max_exact_integer_sample,
                &unbounded(),
                FloatWidth::F64,
            ),
            &[TWO_53, -TWO_53],
        );
        assert_exactly_set(
            &direct_draws(
                float_max_exact_integer_sample,
                &unbounded(),
                FloatWidth::F32,
            ),
            &[TWO_24, -TWO_24],
        );
    }

    #[test]
    fn float_signed_zero_sample_is_an_admitted_zero() {
        assert_exactly_set(
            &direct_draws(float_signed_zero_sample, &unbounded(), FloatWidth::F64),
            &[0.0, -0.0],
        );
        assert_exactly_set(
            &direct_draws(float_signed_zero_sample, &choice(0.0, 1.0), FloatWidth::F64),
            &[0.0],
        );
    }

    #[test]
    fn float_binade_edge_sample_is_a_power_of_two_or_its_predecessor() {
        for v in direct_draws(float_binade_edge_sample, &unbounded(), FloatWidth::F64) {
            let mantissa = v.to_bits() & FLOAT_MANTISSA_MASK;
            assert!(mantissa == 0 || mantissa == FLOAT_MANTISSA_MASK, "{v:e}");
        }
    }

    /// Every finite float is dyadic, so the non-dyadic intent is not observable
    /// from the output; only finiteness, non-integrality and size are checked.
    #[test]
    fn float_non_dyadic_sample_is_a_small_non_integer() {
        for v in direct_draws(float_non_dyadic_sample, &unbounded(), FloatWidth::F64) {
            assert!(v.is_finite() && v != v.trunc() && v.abs() <= 101.0, "{v}");
        }
        for v in direct_draws(
            float_non_dyadic_sample,
            &choice(1000.0, 2000.0),
            FloatWidth::F64,
        ) {
            assert!((1000.0..=2000.0).contains(&v), "{v}");
        }
    }
}

#[test]
fn weighted_index_sample_follows_the_weights_and_skips_excluded_entries() {
    let mut rng = EngineRng::seeded(7);
    let weights = [1.0, 3.0, 0.0, 0.0];
    let mut counts = [0usize; 4];
    for _ in 0..6000 {
        counts[weighted_index_sample(&weights, &mut rng)] += 1;
    }
    assert_eq!(counts[2], 0);
    assert_eq!(counts[3], 0);
    assert!(
        counts[1] > counts[0] * 2 && counts[1] < counts[0] * 4,
        "expected roughly a 1:3 split, got {counts:?}"
    );
}

#[test]
fn weighted_index_sample_of_a_single_positive_entry_is_that_entry() {
    let mut rng = EngineRng::seeded(7);
    for _ in 0..100 {
        assert_eq!(weighted_index_sample(&[0.0, 0.0, 2.0], &mut rng), 2);
    }
}

#[test]
fn draw_index_weighted_records_an_ordinary_index_choice() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(3)).unwrap();
    let mut seen = [false; 3];
    for _ in 0..50 {
        let i = ntc.draw_index_weighted(&[0.0, 5.0, 5.0]).unwrap();
        assert_ne!(i, 0);
        seen[i] = true;
    }
    assert!(seen[1] && seen[2]);
    let node = &ntc.nodes[0];
    assert!(!node.was_forced);
    assert!(matches!(
        node.kind(),
        ChoiceKind::Integer(k) if k.min_value == BigInt::zero() && k.max_value == BigInt::from(2)
    ));
}

#[test]
fn draw_index_weighted_forces_the_only_positive_entry() {
    // prefix that should be ignored
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(0))], None, None);
    assert_eq!(ntc.draw_index_weighted(&[0.0, 5.0, 0.0]).unwrap(), 1);
    let node = &ntc.nodes[0];
    assert!(node.was_forced);
    assert_eq!(node.value(), ChoiceValue::Integer(BigInt::from(1)));
    assert!(matches!(
        node.kind(),
        ChoiceKind::Integer(k) if k.min_value == BigInt::zero() && k.max_value == BigInt::from(2)
    ));
}

#[test]
fn draw_index_weighted_replays_the_prefix_even_for_an_excluded_entry() {
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(0))], None, None);
    assert_eq!(ntc.draw_index_weighted(&[0.0, 1.0, 1.0]).unwrap(), 0);
}

#[test]
fn draw_index_weighted_simplest_is_zero() {
    let mut ntc = NativeTestCase::for_simplest(8).unwrap();
    assert_eq!(ntc.draw_index_weighted(&[0.0, 1.0, 1.0]).unwrap(), 0);
}
