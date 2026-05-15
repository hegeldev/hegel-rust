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
