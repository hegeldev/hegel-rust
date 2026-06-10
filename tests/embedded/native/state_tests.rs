use super::*;
use crate::native::rng::EngineRng;

// ── Spans::get_mut ────────────────────────────────────────────────────────

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

// ── Spans::get_signed ────────────────────────────────────────────────────

#[test]
fn spans_get_signed_positive_index() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 2,
        label: "a".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    spans.push(Span {
        start: 2,
        end: 4,
        label: "b".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert_eq!(spans.get_signed(0).unwrap().label, "a");
    assert_eq!(spans.get_signed(1).unwrap().label, "b");
}

#[test]
fn spans_get_signed_negative_index() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 2,
        label: "first".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    spans.push(Span {
        start: 2,
        end: 4,
        label: "last".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert_eq!(spans.get_signed(-1).unwrap().label, "last");
    assert_eq!(spans.get_signed(-2).unwrap().label, "first");
}

#[test]
fn spans_get_signed_out_of_range_returns_none() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "only".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert!(spans.get_signed(1).is_none());
    assert!(spans.get_signed(-2).is_none());
    assert!(spans.get_signed(100).is_none());
}

// ── Spans::children ───────────────────────────────────────────────────────

#[test]
fn spans_children_returns_direct_children() {
    let mut spans = Spans::new();
    // Span 0: root
    spans.push(Span {
        start: 0,
        end: 10,
        label: "root".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    // Span 1: child of 0
    spans.push(Span {
        start: 0,
        end: 5,
        label: "child1".to_string(),
        depth: 1,
        parent: Some(0),
        discarded: false,
    });
    // Span 2: child of 0
    spans.push(Span {
        start: 5,
        end: 10,
        label: "child2".to_string(),
        depth: 1,
        parent: Some(0),
        discarded: false,
    });
    // Span 3: grandchild of 1
    spans.push(Span {
        start: 0,
        end: 3,
        label: "grandchild".to_string(),
        depth: 2,
        parent: Some(1),
        discarded: false,
    });
    let children = spans.children(0);
    assert_eq!(children, vec![1, 2]);
    let children1 = spans.children(1);
    assert_eq!(children1, vec![3]);
}

// ── Spans::trivial ────────────────────────────────────────────────────────

#[test]
fn spans_trivial_handles_simplest_forced_and_oob() {
    use crate::native::core::choices::{BooleanChoice, ChoiceKind, ChoiceNode, ChoiceValue};
    let kind = ChoiceKind::Boolean(BooleanChoice);
    let simplest = ChoiceNode::new(kind.clone(), ChoiceValue::Boolean(false), false);
    let interesting = ChoiceNode::new(kind.clone(), ChoiceValue::Boolean(true), false);
    let forced_interesting = ChoiceNode::new(kind, ChoiceValue::Boolean(true), true);

    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 2,
        label: "outer".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });

    // Both children simplest → trivial.
    let nodes = vec![simplest.clone(), simplest.clone()];
    assert!(spans.trivial(0, &nodes));

    // A non-forced non-simplest child → not trivial.
    let nodes = vec![simplest.clone(), interesting.clone()];
    assert!(!spans.trivial(0, &nodes));

    // A forced child counts as trivial even if its value isn't simplest.
    let nodes = vec![simplest, forced_interesting];
    assert!(spans.trivial(0, &nodes));

    // Out-of-range span index returns false.
    let other = Spans::new();
    let empty: Vec<ChoiceNode> = Vec::new();
    assert!(!other.trivial(7, &empty));
}

// ── Spans::into_vec ───────────────────────────────────────────────────────

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

// ── From<Vec<Span>> for Spans ──────────────────────────────────────────────

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

// ── Deref for Spans ───────────────────────────────────────────────────────

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
    // Deref: use slice methods
    let slice: &[Span] = &spans;
    assert_eq!(slice.len(), 1);
    assert_eq!(slice[0].label, "deref");
}

// ── IntoIterator for &Spans ───────────────────────────────────────────────

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

// ── Index<usize> for Spans ────────────────────────────────────────────────

#[test]
fn spans_index_usize() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "idx".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert_eq!(spans[0usize].label, "idx");
}

// ── Index<i64> for Spans ─────────────────────────────────────────────────

#[test]
fn spans_index_i64_positive() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "pos".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert_eq!(spans[0i64].label, "pos");
}

#[test]
fn spans_index_i64_negative() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "neg".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    assert_eq!(spans[-1i64].label, "neg");
}

#[test]
#[should_panic(expected = "out of range")]
fn spans_index_i64_out_of_range_panics() {
    let spans = Spans::new();
    let _ = spans[0i64];
}

// ── NativeTestCase::as_result returning Overrun ───────────────────────────

// ── NativeTestCase::draw_integer_forced ───────────────────────────────────

// ── Spans::get (by non-negative usize index) ──────────────────────────────

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

// ── Spans::as_slice ───────────────────────────────────────────────────────

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

// ── Spans::as_mut_slice ───────────────────────────────────────────────────

#[test]
fn spans_as_mut_slice_allows_mutation() {
    let mut spans = Spans::new();
    spans.push(Span {
        start: 0,
        end: 1,
        label: "mutable".to_string(),
        depth: 0,
        parent: None,
        discarded: false,
    });
    {
        let sl = spans.as_mut_slice();
        sl[0].discarded = true;
    }
    assert!(spans.get(0).unwrap().discarded);
}

// ── DataObserver default methods ──────────────────────────────────────────
//
// The default implementations are no-ops; we just need to call them to get
// coverage. A concrete impl that overrides none of the methods suffices.

struct NoopObserver;
impl DataObserver for NoopObserver {}

// ── NativeTestCase::stop_span with empty stack ────────────────────────────

#[test]
fn stop_span_on_empty_stack_is_a_no_op() {
    // If the span_stack is already empty, stop_span must return early
    // without panicking. Covers the `let Some(idx) = ... else { return; }` arm.
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    // No start_span called, so span_stack is empty.
    tc.stop_span(false); // must not panic
    assert!(tc.spans.is_empty());
}

// ── DataObserver default method bodies ────────────────────────────────────
//
// Each default body is a no-op; calling it on a struct that doesn't override
// the method exercises the default arm.

#[test]
fn data_observer_draw_boolean_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_boolean(true, false); // must not panic
}

#[test]
fn data_observer_draw_integer_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_integer(&BigInt::from(42), false); // must not panic
}

#[test]
fn data_observer_draw_float_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_float(1.5, false); // must not panic
}

#[test]
fn data_observer_conclude_test_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.conclude_test(Status::Valid, None); // must not panic
}

// ── NativeTestCase::weighted forces `false` when `p <= 0.0` ──────────────
//
// `weighted`'s `forced.or(...)` chain promotes `p <= 0.0` and `p >= 1.0`
// into forced values without recording an RNG draw.  Test cases that go
// through `many_more` with a closed boundary exercise these.

#[test]
fn weighted_with_p_zero_returns_false_without_consulting_rng() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0));
    // RNG is present but `p == 0.0` is supposed to short-circuit it.
    let v = tc.weighted(0.0, None).ok().unwrap();
    assert!(!v);
    assert!(tc.nodes.last().unwrap().was_forced);
}

#[test]
fn weighted_with_p_one_returns_true_without_consulting_rng() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0));
    let v = tc.weighted(1.0, None).ok().unwrap();
    assert!(v);
    assert!(tc.nodes.last().unwrap().was_forced);
}

#[test]
fn weighted_with_explicit_forced_records_forced_node() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0));
    let v = tc.weighted(0.5, Some(true)).ok().unwrap();
    assert!(v);
    assert!(tc.nodes.last().unwrap().was_forced);
    let v = tc.weighted(0.5, Some(false)).ok().unwrap();
    assert!(!v);
    assert!(tc.nodes.last().unwrap().was_forced);
}

// ── NativeTestCase::weighted notifies the observer on draw ──────────────
//
// The observer hook in `weighted` fires after the boolean is recorded;
// a custom observer captures the value to verify the call site at
// `state.rs:obs.draw_boolean(...)` runs.

// ── NativeTestCase::freeze is idempotent ─────────────────────────────────

#[test]
fn freeze_is_a_no_op_on_already_frozen_test_case() {
    // freeze sets `frozen = true`; calling it again should hit the
    // `if self.frozen { return; }` early return rather than
    // re-running the close-spans / observer-notify path.
    let mut tc = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None, None);
    tc.start_span(7);
    tc.stop_span(false);
    tc.freeze();
    let spans_after_first = tc.spans.clone().into_vec();
    tc.freeze(); // second freeze must be a no-op
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

// ── NativeTestCase::freeze with observer ──────────────────────────────────
//
// When an observer is attached, freeze() must call conclude_test.

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
    // `conclude_test` was called exactly once with the current status —
    // for a never-marked test case that's `Status::Valid` (the default).
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some(Status::Valid));
}

// ── NativeTestCase::draw_integer with observer ─────────────────────────────

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
    // The observer must have captured the prefix-replayed value with
    // was_forced=false.
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((BigInt::from(99), false)));
}

// ── NativeTestCase::draw_float with observer ──────────────────────────────

#[test]
fn draw_float_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct FloatObserver {
        captured: Arc<Mutex<Option<(u64, bool)>>>,
    }
    impl DataObserver for FloatObserver {
        fn draw_float(&mut self, value: f64, was_forced: bool) {
            // Capture the bit pattern so `-0.0` and NaN payloads compare exactly.
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
    obs.draw_bytes(&[1, 2, 3], false); // must not panic
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
    obs.draw_string("hello", false); // must not panic
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
        crate::native::intervalsets::IntervalSet::new(vec![(0, 0xD7FF), (0xE000, 0x10FFFF)]);
    let s = tc.draw_string(intervals, 0, 10).ok().unwrap();
    assert_eq!(s, "abc");
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some(("abc".to_string(), false)));
}

// ── NativeTestCase::stop_span extends parent labels (line 798) ────────────
//
// When stop_span is called with discard=false and there is a parent span
// on labels_for_structure_stack, labels are extended into the parent.

#[test]
fn stop_span_extends_parent_label_stack() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    // Open two nested spans; the inner one's labels get propagated to the outer.
    tc.start_span(1);
    tc.start_span(2);
    // stop_span(false) on the inner span: extends parent with inner's labels.
    tc.stop_span(false);
    // stop_span(false) on the outer span: extends tags.
    tc.stop_span(false);
    // No panic means the label propagation path was executed.
}

// ── draw_float on a fresh random NTC ─────────────────────────────────────

#[test]
fn draw_float_unbounded_with_nan_can_produce_nan() {
    // Fully unbounded with allow_nan=true exercises the random-generation
    // branch including the NaN-emission arm.
    for seed in 0..200u64 {
        let mut tc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let v = tc
            .draw_float(f64::NEG_INFINITY, f64::INFINITY, true, true, 5e-324)
            .ok()
            .unwrap();
        if v.is_nan() {
            return; // exercised
        }
    }
    panic!("never produced NaN in 200 unbounded draws with allow_nan=true");
}

#[test]
fn draw_float_half_bounded_below_explores_finite_range() {
    let mut tc = NativeTestCase::new_random(EngineRng::seeded(0));
    let v = tc
        .draw_float(1.0, f64::INFINITY, false, false, 5e-324)
        .ok()
        .unwrap();
    assert!(v >= 1.0 && !v.is_nan());
}

// ── NativeTestCase::for_simplest ─────────────────────────────────────────────
//
// The all-simplest pre-trial run at the head of the Generate phase. Every
// draw must return the kind's `simplest()` value — `shrink_towards`
// clamped to range for integers, 0.0 for floats, false for booleans, the
// empty / lower-bound size for bytes and strings.

#[test]
fn for_simplest_draws_integer_at_shrink_target_when_in_range() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    // shrink_towards=0 is hardcoded; for [0, 23] that's in range → simplest = 0.
    let v = tc.draw_integer::<i128>(0, 23).ok().unwrap();
    assert_eq!(v, 0);
}

#[test]
fn for_simplest_draws_integer_clamped_to_range_when_target_below() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    // shrink_towards=0 below min=5 → simplest clamps to 5.
    let v = tc.draw_integer::<i128>(5, 100).ok().unwrap();
    assert_eq!(v, 5);
}

#[test]
fn for_simplest_draws_integer_clamped_to_range_when_target_above() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    // shrink_towards=0 above max=-1 → simplest clamps to -1.
    let v = tc.draw_integer::<i128>(-100, -1).ok().unwrap();
    assert_eq!(v, -1);
}

#[test]
fn for_simplest_propagates_across_many_draws() {
    // The mode applies to every draw, not just the first. This is what
    // makes "midnight = all four time components are zero" findable on
    // a single pre-trial.
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    for _ in 0..10 {
        assert_eq!(tc.draw_integer::<i128>(0, 99).ok().unwrap(), 0);
    }
}

#[test]
fn for_simplest_draws_float_at_zero() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    let v = tc
        .draw_float(-10.0, 10.0, false, false, 5e-324)
        .ok()
        .unwrap();
    assert_eq!(v, 0.0);
    assert!(v.is_sign_positive(), "expected +0.0, got -0.0");
}

#[test]
fn for_simplest_draws_weighted_at_false() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    let v = tc.weighted(0.5, None).ok().unwrap();
    assert!(!v, "weighted draw in simplest mode should be false");
}

#[test]
fn for_simplest_draws_bytes_at_min_size_all_zero() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    let v = tc.draw_bytes(2, 5).ok().unwrap();
    assert_eq!(v, vec![0u8; 2], "expected min-sized all-zero buffer");
}

#[test]
fn for_simplest_is_independent_of_seed() {
    // Two simplest test cases produce identical traces — that's the whole
    // point: deterministic boundary trial that doesn't depend on RNG luck.
    let mut a = NativeTestCase::for_simplest(BUFFER_SIZE);
    let mut b = NativeTestCase::for_simplest(BUFFER_SIZE);
    for _ in 0..5 {
        let va = a.draw_integer::<i128>(0, 1000).ok().unwrap();
        let vb = b.draw_integer::<i128>(0, 1000).ok().unwrap();
        assert_eq!(va, vb);
        assert_eq!(va, 0);
    }
}

#[test]
fn for_simplest_records_choice_nodes() {
    // Forced-simplest draws still record nodes in the test case so the
    // engine can inspect what was drawn and feed the trace into the data
    // tree / shrinker if the test ends up Interesting.
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    let _ = tc.draw_integer::<i128>(0, 23).ok().unwrap();
    let _ = tc.weighted(0.5, None).ok().unwrap();
    assert_eq!(tc.nodes.len(), 2);
}

// ── ChoiceTemplate / trailing_template ───────────────────────────────────────
//
// Direct tests for the new template mechanism. The for_simplest_* tests above
// exercise the same paths through the `for_simplest` wrapper; these target
// the underlying `for_choices_and_template` constructor and the count /
// mixed-prefix behaviours that wrapper hides.

#[test]
fn template_simplest_infinite_resolves_every_draw_to_simplest() {
    let mut tc = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(None)),
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
        Some(ChoiceTemplate::simplest(Some(3))),
        100,
        None,
    );
    for _ in 0..3 {
        assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
    }
    // 4th draw is the overrun edge: returns Err and sets EarlyStop.
    assert!(tc.draw_integer::<i128>(0, 100).is_err());
    assert_eq!(tc.status, Some(Status::EarlyStop));
}

#[test]
fn template_concrete_prefix_then_template() {
    let prefix = vec![ChoiceValue::Integer(BigInt::from(42))];
    let mut tc = NativeTestCase::for_choices_and_template(
        &prefix,
        None,
        Some(ChoiceTemplate::simplest(None)),
        10,
        None,
    );
    // First draw replays the concrete prefix entry.
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 42);
    // Subsequent draws fall through to the template → simplest().
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
}

#[test]
fn template_concrete_prefix_with_punning_then_template() {
    // Prefix was originally a Boolean, but the test is drawing an Integer:
    // punning routes the first draw to unit() (since the original wasn't
    // "simplest"), and the template kicks in for subsequent draws.
    let prefix = vec![ChoiceValue::Boolean(true)];
    let prefix_nodes = vec![ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(true),
        false,
    )];
    let mut tc = NativeTestCase::for_choices_and_template(
        &prefix,
        Some(&prefix_nodes),
        Some(ChoiceTemplate::simplest(None)),
        10,
        None,
    );
    // Draw 1: kind mismatch + non-simplest original → unit().
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
    // Draw 2: template branch → simplest().
    assert_eq!(tc.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
}

#[test]
#[should_panic(expected = "ChoiceTemplate count must be positive")]
fn template_count_zero_panics_at_construction() {
    let _ = ChoiceTemplate::simplest(Some(0));
}

#[test]
fn for_simplest_wrapper_matches_template_with_count_none() {
    // for_simplest is just sugar for the explicit template; identical traces.
    let mut a = NativeTestCase::for_simplest(5);
    let mut b = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(None)),
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
fn template_overrun_status_matches_max_size_overrun() {
    // Finite-count exhaustion and max_size exhaustion both set Status::EarlyStop —
    // overrun behaviour is uniform across both paths.
    let mut tc_count = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(Some(2))),
        100,
        None,
    );
    assert_eq!(tc_count.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
    assert_eq!(tc_count.draw_integer::<i128>(0, 100).ok().unwrap(), 0);
    assert!(tc_count.draw_integer::<i128>(0, 100).is_err());
    assert_eq!(tc_count.status, Some(Status::EarlyStop));
}

#[test]
fn template_count_decrements_on_each_draw() {
    // White-box check: count walks 3 → 2 → 1 → 0 across three draws, then
    // the fourth draw flips to overrun without further decrement.
    let mut tc = NativeTestCase::for_choices_and_template(
        &[],
        None,
        Some(ChoiceTemplate::simplest(Some(3))),
        100,
        None,
    );
    for _ in 0..3 {
        let _ = tc.draw_integer::<i128>(0, 100).ok().unwrap();
    }
    assert_eq!(tc.trailing_template.as_ref().unwrap().count, Some(0));
    assert!(tc.draw_integer::<i128>(0, 100).is_err());
    // After overrun, count stays at 0 (we don't underflow into wraparound).
    assert_eq!(tc.trailing_template.as_ref().unwrap().count, Some(0));
}

// ── biased_integer_sample / new piecewise distribution ────────────────────

#[test]
fn biased_integer_sample_stays_in_range_for_small_bounds() {
    let mut rng = EngineRng::seeded(1);
    for _ in 0..1000 {
        let v = biased_i128_sample(0, 100, &mut rng);
        assert!((0..=100).contains(&v), "out of range: {v}");
    }
}

#[test]
fn biased_integer_sample_stays_in_range_for_wide_bounds() {
    let mut rng = EngineRng::seeded(2);
    for _ in 0..2000 {
        let v = biased_i128_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
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
        // Range is the whole i128 domain, so any returned value is in range;
        // the assertion is implicit (no panic / overflow).
        let _ = biased_i128_sample(i128::MIN, i128::MAX, &mut rng);
    }
}

#[test]
fn biased_integer_sample_collapses_when_min_equals_max() {
    let mut rng = EngineRng::seeded(4);
    for _ in 0..100 {
        assert_eq!(biased_i128_sample(42, 42, &mut rng), 42);
    }
}

#[test]
fn biased_integer_sample_produces_diverse_magnitudes_unbounded() {
    // The piecewise distribution should produce values across many orders of
    // magnitude when the range is wide.
    let mut rng = EngineRng::seeded(5);
    let mut magnitudes: HashSet<i32> = HashSet::new();
    for _ in 0..2000 {
        let v = biased_i128_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
        // bucket by bit-length of |v|
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
        let v = biased_i128_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
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
    // The full i64 range has hundreds of in-range nasty-pool entries; the
    // pool probability must be capped so that the piecewise distribution
    // still runs (uncapped, `count * BOUNDARY_PROBABILITY` exceeded 1 and
    // every draw came from the pool).
    let mut rng = EngineRng::seeded(8);
    let pool = &*SORTED_NASTY_POOL;
    let total = 2000;
    let mut outside_pool = 0;
    for _ in 0..total {
        let v = biased_i128_sample(i64::MIN as i128, i64::MAX as i128, &mut rng);
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
        .map(|_| biased_i128_sample(10_000, 10_000_000, &mut rng))
        .collect();
    samples.sort();
    let median = samples[samples.len() / 2];
    assert!(
        median < 1_000_000,
        "median {median} is too high; expected log-skewed distribution"
    );
}

#[test]
fn biased_string_sample_caps_constant_pool_probability() {
    // With a permissive full-Unicode alphabet every global string constant
    // validates, so an uncapped per-candidate threshold would send ~60% of
    // draws to the constant pool instead of the alphabet-driven sampler.
    let sc = StringChoice {
        intervals: crate::native::intervalsets::IntervalSet::new(vec![
            (0, 0xD7FF),
            (0xE000, 0x10FFFF),
        ]),
        min_size: 0,
        max_size: 100,
    };
    let mut rng = EngineRng::seeded(9);
    let pool = &*GLOBAL_CONSTANTS_STRINGS;
    let total = 2000;
    let mut from_pool = 0;
    for _ in 0..total {
        let v = biased_string_sample(&sc, &mut rng);
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
fn biased_float_sample_full_finite_range_does_not_collapse_to_max() {
    // `gs::floats().allow_nan(false).allow_infinity(false)` produces exactly
    // this choice. The legacy uniform draw computed `min + r * (max - min)`,
    // where the range width overflows to +inf, collapsing ~90% of draws to
    // exactly `f64::MAX`.
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
        let v = biased_float_sample(&fc, &mut rng);
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
    // The Hypothesis-style lex draw puts about half its mass on integers
    // below 2^56; require a healthy share of simple integer-valued floats.
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
        let v = biased_i128_sample(0, 1, &mut rng);
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

// ── Erased `biased_integer_sample` over `IntegerChoice` ─────────────────

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
        let v = biased_integer_sample(&kind, &mut rng);
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
        let v = biased_integer_sample(&kind, &mut rng);
        assert!(kind.validate(&v), "out of range: {v:?}");
    }
}

#[test]
fn integer_sample_from_distribution_uniform_fallback_for_indistinguishable_bounds() {
    // At the extreme tail of i128, `min as f64` and `max as f64` lose
    // precision and round to the same value. The CDF window is then 0,
    // which is below the 1e-13 threshold and forces the uniform fallback
    // (the only path that produces a value in [min, max] when the
    // distribution-based path can't distinguish the endpoints).
    let mut rng = EngineRng::seeded(13);
    let min = i128::MAX - 1000;
    let max = i128::MAX;
    let mut all_endpoints = true;
    for _ in 0..50 {
        let v = integer_sample_from_distribution(min, max, &mut rng);
        assert!(v >= min && v <= max, "out of range: {v}");
        if v != min && v != max {
            all_endpoints = false;
        }
    }
    // Uniform should produce interior values, not collapse to endpoints —
    // distinguishes the fallback path from the inverse-CDF path (which
    // would saturate to one endpoint when the CDF window is degenerate).
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
        assert_eq!(biased_integer_sample(&kind, &mut rng), fixed.clone());
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
    // The decision compares the single drawn byte against the falsey threshold.
    let falsey = (256.0_f64 * (1.0 - 0.5)).floor().max(1.0) as u32; // 128
    assert_eq!(result, u32::from(byte[0]) >= falsey);
    // Exactly one byte was consumed: the two RNGs are now in lockstep, which
    // would not hold had the draw read a u32 (4 bytes) or an f64 (8 bytes).
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
    // A remapped draw landing in (0, smallest_nonzero_magnitude) defaults to
    // the smallest allowed magnitude (make_float_clamper's re-route).
    let fc = FloatChoice {
        min_value: -1e-307,
        max_value: 1e-307,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    // Mantissa fraction ~0.5 lands the remap just below zero, inside the
    // excluded band.
    let raw = f64::from_bits(((1u64 << 52) - 1) / 2);
    let clamped = float_clamp(&fc, raw);
    assert_eq!(clamped, f64::MIN_POSITIVE);

    // When the smallest allowed magnitude exceeds max_value, only its
    // negation is in range.
    let fc_neg = FloatChoice {
        min_value: -1e-307,
        max_value: -1e-308,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    };
    // Mantissa fraction ~0.9: remap lands at ~-1.9e-308, inside the band.
    let raw_neg = f64::from_bits((((1u64 << 52) - 1) / 10) * 9);
    let clamped_neg = float_clamp(&fc_neg, raw_neg);
    assert_eq!(clamped_neg, -f64::MIN_POSITIVE);
}
