use super::*;
use rand::SeedableRng;
use rand::rngs::SmallRng;

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
    let simplest = ChoiceNode {
        kind: kind.clone(),
        value: ChoiceValue::Boolean(false),
        was_forced: false,
    };
    let interesting = ChoiceNode {
        kind: kind.clone(),
        value: ChoiceValue::Boolean(true),
        was_forced: false,
    };
    let forced_interesting = ChoiceNode {
        kind,
        value: ChoiceValue::Boolean(true),
        was_forced: true,
    };

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
    obs.draw_integer(42, false); // must not panic
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
    let mut tc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    // RNG is present but `p == 0.0` is supposed to short-circuit it.
    let v = tc.weighted(0.0, None).ok().unwrap();
    assert!(!v);
    assert!(tc.nodes.last().unwrap().was_forced);
}

#[test]
fn weighted_with_p_one_returns_true_without_consulting_rng() {
    let mut tc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    let v = tc.weighted(1.0, None).ok().unwrap();
    assert!(v);
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
        captured: Arc<Mutex<Option<(i128, bool)>>>,
    }
    impl DataObserver for IntObserver {
        fn draw_integer(&mut self, value: i128, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value, was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::Integer(99)];
    let obs = Box::new(IntObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_integer(0, 100).ok().unwrap();
    assert_eq!(v, 99);
    // The observer must have captured the prefix-replayed value with
    // was_forced=false.
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((99, false)));
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
    let v = tc.draw_float(0.0, 10.0, false, false).ok().unwrap();
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
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    // Fully unbounded with allow_nan=true exercises the random-generation
    // branch including the NaN-emission arm.
    for seed in 0..200u64 {
        let mut tc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
        let v = tc
            .draw_float(f64::NEG_INFINITY, f64::INFINITY, true, true)
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
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    let mut tc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    let v = tc
        .draw_float(1.0, f64::INFINITY, false, false)
        .ok()
        .unwrap();
    assert!(v >= 1.0 && !v.is_nan());
}

// ── NativeTestCase::for_simplest ─────────────────────────────────────────────
//
// Mirrors the `cached_test_function((ChoiceTemplate("simplest", count=None),))`
// pre-trial that Hypothesis's engine runs at the head of the Generate phase
// (engine.py::generate_new_examples). Every draw must return the kind's
// `simplest()` value — `shrink_towards` clamped to range for integers, 0.0
// for floats, false for booleans, the empty / lower-bound size for bytes
// and strings.

#[test]
fn for_simplest_draws_integer_at_shrink_target_when_in_range() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    // shrink_towards=0 is hardcoded; for [0, 23] that's in range → simplest = 0.
    let v = tc.draw_integer(0, 23).ok().unwrap();
    assert_eq!(v, 0);
}

#[test]
fn for_simplest_draws_integer_clamped_to_range_when_target_below() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    // shrink_towards=0 below min=5 → simplest clamps to 5.
    let v = tc.draw_integer(5, 100).ok().unwrap();
    assert_eq!(v, 5);
}

#[test]
fn for_simplest_draws_integer_clamped_to_range_when_target_above() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    // shrink_towards=0 above max=-1 → simplest clamps to -1.
    let v = tc.draw_integer(-100, -1).ok().unwrap();
    assert_eq!(v, -1);
}

#[test]
fn for_simplest_propagates_across_many_draws() {
    // The mode applies to every draw, not just the first. This is what makes
    // Hypothesis-style "midnight = all four time components are zero" findable
    // on a single pre-trial.
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    for _ in 0..10 {
        assert_eq!(tc.draw_integer(0, 99).ok().unwrap(), 0);
    }
}

#[test]
fn for_simplest_draws_float_at_zero() {
    let mut tc = NativeTestCase::for_simplest(BUFFER_SIZE);
    let v = tc.draw_float(-10.0, 10.0, false, false).ok().unwrap();
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
        let va = a.draw_integer(0, 1000).ok().unwrap();
        let vb = b.draw_integer(0, 1000).ok().unwrap();
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
    let _ = tc.draw_integer(0, 23).ok().unwrap();
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
        assert_eq!(tc.draw_integer(-100, 100).ok().unwrap(), 0);
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
        assert_eq!(tc.draw_integer(0, 100).ok().unwrap(), 0);
    }
    // 4th draw is the overrun edge: returns Err and sets EarlyStop.
    assert!(tc.draw_integer(0, 100).is_err());
    assert_eq!(tc.status, Some(Status::EarlyStop));
}

#[test]
fn template_concrete_prefix_then_template() {
    let prefix = vec![ChoiceValue::Integer(42)];
    let mut tc = NativeTestCase::for_choices_and_template(
        &prefix,
        None,
        Some(ChoiceTemplate::simplest(None)),
        10,
        None,
    );
    // First draw replays the concrete prefix entry.
    assert_eq!(tc.draw_integer(0, 100).ok().unwrap(), 42);
    // Subsequent draws fall through to the template → simplest().
    assert_eq!(tc.draw_integer(0, 100).ok().unwrap(), 0);
    assert_eq!(tc.draw_integer(0, 100).ok().unwrap(), 0);
}

#[test]
fn template_concrete_prefix_with_punning_then_template() {
    // Prefix was originally a Boolean, but the test is drawing an Integer:
    // punning routes the first draw to unit() (since the original wasn't
    // "simplest"), and the template kicks in for subsequent draws.
    let prefix = vec![ChoiceValue::Boolean(true)];
    let prefix_nodes = vec![ChoiceNode {
        kind: ChoiceKind::Boolean(BooleanChoice),
        value: ChoiceValue::Boolean(true),
        was_forced: false,
    }];
    let mut tc = NativeTestCase::for_choices_and_template(
        &prefix,
        Some(&prefix_nodes),
        Some(ChoiceTemplate::simplest(None)),
        10,
        None,
    );
    // Draw 1: kind mismatch + non-simplest original → unit().
    let v = tc.draw_integer(-100, 100).ok().unwrap();
    assert_eq!(
        v,
        IntegerChoice {
            min_value: -100,
            max_value: 100,
            shrink_towards: 0,
        }
        .unit()
    );
    // Draw 2: template branch → simplest().
    assert_eq!(tc.draw_integer(0, 100).ok().unwrap(), 0);
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
        let va = a.draw_integer(-10, 10).ok().unwrap();
        let vb = b.draw_integer(-10, 10).ok().unwrap();
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
    assert_eq!(tc_count.draw_integer(0, 100).ok().unwrap(), 0);
    assert_eq!(tc_count.draw_integer(0, 100).ok().unwrap(), 0);
    assert!(tc_count.draw_integer(0, 100).is_err());
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
        let _ = tc.draw_integer(0, 100).ok().unwrap();
    }
    assert_eq!(tc.trailing_template.as_ref().unwrap().count, Some(0));
    assert!(tc.draw_integer(0, 100).is_err());
    // After overrun, count stays at 0 (we don't underflow into wraparound).
    assert_eq!(tc.trailing_template.as_ref().unwrap().count, Some(0));
}

// ── biased_integer_sample / new piecewise distribution ────────────────────

fn ic(min_value: i128, max_value: i128) -> IntegerChoice {
    IntegerChoice {
        min_value,
        max_value,
        shrink_towards: 0,
    }
}

#[test]
fn biased_integer_sample_stays_in_range_for_small_bounds() {
    let kind = ic(0, 100);
    let mut rng = SmallRng::seed_from_u64(1);
    for _ in 0..1000 {
        let v = biased_integer_sample(&kind, &mut rng);
        assert!(kind.validate(v), "out of range: {v}");
    }
}

#[test]
fn biased_integer_sample_stays_in_range_for_wide_bounds() {
    let kind = ic(i64::MIN as i128, i64::MAX as i128);
    let mut rng = SmallRng::seed_from_u64(2);
    for _ in 0..2000 {
        let v = biased_integer_sample(&kind, &mut rng);
        assert!(kind.validate(v), "out of range: {v}");
    }
}

#[test]
fn biased_integer_sample_stays_in_range_for_full_i128() {
    let kind = ic(i128::MIN, i128::MAX);
    let mut rng = SmallRng::seed_from_u64(3);
    for _ in 0..1000 {
        let v = biased_integer_sample(&kind, &mut rng);
        assert!(kind.validate(v), "out of range: {v}");
    }
}

#[test]
fn biased_integer_sample_collapses_when_min_equals_max() {
    let kind = ic(42, 42);
    let mut rng = SmallRng::seed_from_u64(4);
    for _ in 0..100 {
        assert_eq!(biased_integer_sample(&kind, &mut rng), 42);
    }
}

#[test]
fn biased_integer_sample_produces_diverse_magnitudes_unbounded() {
    // The new piecewise distribution should produce values across many
    // orders of magnitude when the range is wide. Use a generous bound on
    // how many distinct magnitudes we should see — the test is about
    // avoiding the prior collapse-to-uniform behaviour, not about exact
    // shape.
    let kind = ic(i64::MIN as i128, i64::MAX as i128);
    let mut rng = SmallRng::seed_from_u64(5);
    let mut magnitudes: HashSet<i32> = HashSet::new();
    for _ in 0..2000 {
        let v = biased_integer_sample(&kind, &mut rng);
        // bucket by bit-length of |v|
        let mag = if v == 0 {
            0
        } else {
            128 - v.unsigned_abs().leading_zeros() as i32
        };
        magnitudes.insert(mag);
    }
    // A uniform-only distribution on [i64::MIN, i64::MAX] would land almost
    // every sample in the top few bits, hitting maybe 3-5 distinct buckets.
    // The piecewise log-student-t should comfortably exceed that.
    assert!(
        magnitudes.len() >= 10,
        "expected >= 10 magnitude buckets, got {}",
        magnitudes.len()
    );
}

#[test]
fn biased_integer_sample_concentrates_around_zero_when_unbounded() {
    // Inner uniform on [-256, 256] gets non-trivial probability mass.
    // Across many samples we should see values land inside [-256, 256]
    // far more often than chance for the wider range would suggest.
    let kind = ic(i64::MIN as i128, i64::MAX as i128);
    let mut rng = SmallRng::seed_from_u64(6);
    let mut in_inner = 0;
    let total = 2000;
    for _ in 0..total {
        let v = biased_integer_sample(&kind, &mut rng);
        if v.unsigned_abs() <= 256 {
            in_inner += 1;
        }
    }
    // For a strictly uniform distribution on [i64::MIN, i64::MAX], the
    // probability of |v| <= 256 is ~513/2^64 ≈ 0. We require at least
    // 5% to confirm the piecewise distribution is doing its job.
    let fraction = in_inner as f64 / total as f64;
    assert!(
        fraction > 0.05,
        "only {fraction} fraction in [-256, 256]; piecewise distribution not active"
    );
}

#[test]
fn biased_integer_sample_log_skewed_bounded_range_favours_smaller_magnitudes() {
    // For a range that doesn't include 0 (so the inner uniform branch
    // doesn't dominate) and avoids most of the constants pool, the
    // restricted log-student-t should favour values near the smaller
    // end. The old uniform-fallback would produce a near-uniform
    // distribution on the range, so its median would land near the
    // midpoint.
    let kind = ic(10_000, 10_000_000);
    let mut rng = SmallRng::seed_from_u64(11);
    let mut samples: Vec<i128> = (0..2000)
        .map(|_| biased_integer_sample(&kind, &mut rng))
        .collect();
    samples.sort();
    let median = samples[samples.len() / 2];
    // Midpoint of the uniform fallback would be ~5_000_000. The new
    // distribution should land well below that. We use 1_000_000 as a
    // generous upper bound that the uniform fallback would clear with
    // overwhelming probability.
    assert!(
        median < 1_000_000,
        "median {median} is too high; expected log-skewed distribution"
    );
}

#[test]
fn biased_integer_sample_narrow_range_uses_uniform_fallback() {
    // Very-narrow ranges where the CDF window is below the 1e-13 threshold
    // should fall back to uniform sampling. min == max - 1 is the
    // smallest non-trivial range; this test exercises the fallback.
    let kind = ic(0, 1);
    let mut rng = SmallRng::seed_from_u64(7);
    let mut seen_zero = false;
    let mut seen_one = false;
    for _ in 0..200 {
        let v = biased_integer_sample(&kind, &mut rng);
        assert!(kind.validate(v), "out of range: {v}");
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

#[test]
fn integer_sample_from_distribution_uniform_fallback_for_indistinguishable_bounds() {
    // At the extreme tail of i128, `min as f64` and `max as f64` lose
    // precision and round to the same value. The CDF window is then 0,
    // which is below the 1e-13 threshold and forces the uniform fallback
    // (the only path that produces a value in [min, max] when the
    // distribution-based path can't distinguish the endpoints).
    let mut rng = SmallRng::seed_from_u64(13);
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
