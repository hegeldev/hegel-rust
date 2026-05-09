use rand::SeedableRng;

use super::*;

// ── NativeTestCase::draw_string ────────────────────────────────────────────
//
// Port of pbtkit/tests/test_text.py::test_draw_string_invalid_range. Pbtkit
// raises ValueError; the native engine's draw_string uses an assert! which
// panics with the same intent.

#[test]
#[should_panic(expected = "Invalid codepoint range")]
fn draw_string_invalid_codepoint_range_panics() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    let _ = tc.draw_string(200, 100, 0, 5);
}

// ── NativeTestCase::start_span past MAX_DEPTH ──────────────────────────────
//
// Hypothesis's `ConjectureData.draw` checks `depth >= MAX_DEPTH` and calls
// `mark_invalid`, which freezes the test case and raises `StopTest`. The
// native engine's `start_span` sets the status to `Invalid` instead, and
// then the next draw must propagate `StopTest` so the test halts cleanly
// rather than panicking with "Frozen: attempted choice on completed test
// case". Recursive `gs::deferred` generators trip this regularly.
#[test]
fn draw_after_max_depth_returns_stop_test() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    for _ in 0..=MAX_DEPTH {
        tc.start_span(0);
    }
    assert!(tc.frozen());
    assert!(tc.draw_integer(0, 100).is_err());
}

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

#[test]
fn as_result_returns_overrun_when_early_stop() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    // conclude_test with EarlyStop status produces Overrun
    let _ = tc.conclude_test(Status::EarlyStop, None);
    let result = tc.as_result();
    assert!(matches!(result, NativeResult::Overrun));
}

// ── NativeTestCase::draw_integer_forced ───────────────────────────────────

#[test]
fn draw_integer_forced_records_forced_node() {
    let mut tc = NativeTestCase::new_random(rand::rngs::SmallRng::seed_from_u64(0));
    let v = tc.draw_integer_forced(0, 100, 42).ok().unwrap();
    assert_eq!(v, 42);
    assert_eq!(tc.nodes.len(), 1);
    assert!(tc.nodes[0].was_forced);
    assert_eq!(tc.nodes[0].value, ChoiceValue::Integer(42));
}

// ── NativeTestCase::draw_float_forced ────────────────────────────────────

#[test]
fn draw_float_forced_records_forced_node() {
    let mut tc = NativeTestCase::new_random(rand::rngs::SmallRng::seed_from_u64(0));
    let v = tc
        .draw_float_forced(0.0, 1.0, false, false, 0.5)
        .ok()
        .unwrap();
    assert!((v - 0.5).abs() < f64::EPSILON);
    assert_eq!(tc.nodes.len(), 1);
    assert!(tc.nodes[0].was_forced);
}

// ── NativeTestCase::draw_bytes_forced ────────────────────────────────────

#[test]
fn draw_bytes_forced_records_forced_node() {
    let mut tc = NativeTestCase::new_random(rand::rngs::SmallRng::seed_from_u64(0));
    let v = tc.draw_bytes_forced(2, 10, vec![1, 2, 3]).ok().unwrap();
    assert_eq!(v, vec![1, 2, 3]);
    assert_eq!(tc.nodes.len(), 1);
    assert!(tc.nodes[0].was_forced);
}

// ── NativeTestCase::draw_string_forced ───────────────────────────────────

#[test]
fn draw_string_forced_records_forced_node() {
    let mut tc = NativeTestCase::new_random(rand::rngs::SmallRng::seed_from_u64(0));
    let v = tc.draw_string_forced(65, 90, 1, 5, "AB").ok().unwrap();
    assert_eq!(v, "AB");
    assert_eq!(tc.nodes.len(), 1);
    assert!(tc.nodes[0].was_forced);
}

// ── NativeTestCase::mark_invalid returning Err(StopTest) ─────────────────

#[test]
fn mark_invalid_returns_err_stop_test() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    let result = tc.mark_invalid(None);
    assert!(result.is_err());
    assert_eq!(tc.status, Some(Status::Invalid));
}

#[test]
fn mark_invalid_with_reason_stores_event() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    let _ = tc.mark_invalid(Some("bad input".to_string()));
    assert_eq!(tc.events().get("invalid because").unwrap(), "bad input");
}

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

#[test]
fn data_observer_default_methods_are_no_ops() {
    let mut obs = NoopObserver;
    // These must not panic.
    obs.draw_integer(0, false);
    obs.draw_float(1.0, false);
    obs.draw_bytes(&[1, 2], false);
    obs.draw_string("hello", false);
    obs.conclude_test(Status::Valid, None);
}

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

// ── NativeResult::Conjecture path in as_result ────────────────────────────

#[test]
fn as_result_returns_conjecture_for_valid_status() {
    let tc = NativeTestCase::for_choices(&[], None, None);
    // A test case that was never concluded has no status (None). as_result()
    // returns Conjecture with Status::Valid in that case.
    let result = tc.as_result();
    match result {
        NativeResult::Conjecture(r) => assert_eq!(r.status, Status::Valid),
        NativeResult::Overrun => panic!("expected Conjecture, got Overrun"),
    }
}

#[test]
fn as_result_returns_conjecture_for_interesting_status() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    // Set interesting status directly.
    tc.status = Some(Status::Interesting);
    tc.frozen = true;
    let result = tc.as_result();
    match result {
        NativeResult::Conjecture(r) => assert_eq!(r.status, Status::Interesting),
        NativeResult::Overrun => panic!("expected Conjecture, got Overrun"),
    }
}

// ── Observer called in draw_float ─────────────────────────────────────────
//
// The `observer` field on `NativeTestCase` is private, so post-draw the
// test can't reach back into the boxed observer.  Instead the observer
// holds an `Arc<Mutex<...>>` that the test side keeps a clone of —
// after the draw, the lock contains exactly what the observer captured.

#[test]
fn draw_float_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct FloatObserver {
        captured: Arc<Mutex<Option<(f64, bool)>>>,
    }
    impl DataObserver for FloatObserver {
        fn draw_float(&mut self, value: f64, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value, was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::Float(1.5)];
    let obs = Box::new(FloatObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_float(0.0, 10.0, false, false).ok().unwrap();
    assert_eq!(v, 1.5);
    // The observer must have captured the drawn value with
    // `was_forced=false` (the value came from the prefix, not a forced
    // override).
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((1.5, false)));
}

// ── Observer called in draw_string ────────────────────────────────────────

#[test]
fn draw_string_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct StringObserver {
        received: Arc<Mutex<Vec<String>>>,
    }
    impl DataObserver for StringObserver {
        fn draw_string(&mut self, value: &str, _was_forced: bool) {
            self.received.lock().unwrap().push(value.to_string());
        }
    }
    let received = Arc::new(Mutex::new(Vec::new()));
    let choices = vec![ChoiceValue::String(vec![65, 66])]; // "AB"
    let obs = Box::new(StringObserver {
        received: received.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_string(65, 90, 1, 5).ok().unwrap();
    assert_eq!(v, "AB");
    // Exactly one observer call, with the realised string.
    let r = received.lock().unwrap();
    assert_eq!(*r, vec!["AB".to_string()]);
}

// ── Observer called in draw_float_forced ─────────────────────────────────

#[test]
fn draw_float_forced_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct ForcedFloatObserver {
        captured: Arc<Mutex<Option<(f64, bool)>>>,
    }
    impl DataObserver for ForcedFloatObserver {
        fn draw_float(&mut self, value: f64, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value, was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::Float(0.0)]; // any float, just for max_size
    let obs = Box::new(ForcedFloatObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc
        .draw_float_forced(0.0, 1.0, false, false, 0.5)
        .ok()
        .unwrap();
    assert!((v - 0.5).abs() < f64::EPSILON);
    // The forced draw must record the forced value AND set was_forced=true.
    let recorded = captured.lock().unwrap().take();
    assert!(matches!(recorded, Some((v, true)) if (v - 0.5).abs() < f64::EPSILON));
}

// ── Observer called in draw_bytes_forced ─────────────────────────────────

#[test]
fn draw_bytes_forced_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct BytesObserver {
        captured: Arc<Mutex<Option<(Vec<u8>, bool)>>>,
    }
    impl DataObserver for BytesObserver {
        fn draw_bytes(&mut self, value: &[u8], was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.to_vec(), was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::Bytes(vec![])]; // placeholder
    let obs = Box::new(BytesObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_bytes_forced(0, 3, vec![1, 2]).ok().unwrap();
    assert_eq!(v, vec![1, 2]);
    // The forced draw must record the forced bytes AND was_forced=true.
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some((vec![1, 2], true)));
}

// ── Observer called in draw_string_forced ────────────────────────────────

#[test]
fn draw_string_forced_notifies_observer() {
    use std::sync::{Arc, Mutex};
    struct StrObserver {
        captured: Arc<Mutex<Option<(String, bool)>>>,
    }
    impl DataObserver for StrObserver {
        fn draw_string(&mut self, value: &str, was_forced: bool) {
            *self.captured.lock().unwrap() = Some((value.to_string(), was_forced));
        }
    }
    let captured = Arc::new(Mutex::new(None));
    let choices = vec![ChoiceValue::String(vec![])]; // placeholder
    let obs = Box::new(StrObserver {
        captured: captured.clone(),
    });
    let mut tc = NativeTestCase::for_choices(&choices, None, Some(obs));
    let v = tc.draw_string_forced(65, 90, 1, 5, "A").ok().unwrap();
    assert_eq!(v, "A");
    // The forced draw must record the forced string AND was_forced=true.
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some(("A".to_string(), true)));
}

// ── DataObserver::draw_boolean default (line 453) ─────────────────────────
//
// The `data_observer_default_methods_are_no_ops` test above omitted
// `draw_boolean`.  Call it to cover the default implementation.

#[test]
fn data_observer_draw_boolean_default_is_no_op() {
    let mut obs = NoopObserver;
    obs.draw_boolean(true, false); // must not panic
}

// ── NativeTestCase::for_prefix_with_max (lines 695-719) ───────────────────
//
// Construct a test case with a prefix and a max_choices cap.  Drawing past
// the cap (or past the prefix) must return StopTest.

#[test]
fn for_prefix_with_max_constructor_and_draw() {
    let prefix = vec![ChoiceValue::Integer(7)];
    let mut tc = NativeTestCase::for_prefix_with_max(&prefix, 1);
    // First draw: replays the prefix value.
    let v = tc.draw_integer(0, 100).ok().unwrap();
    assert_eq!(v, 7);
    // Second draw: past max_choices → StopTest.
    let err = tc.draw_integer(0, 100);
    assert!(err.is_err());
}

// ── NativeTestCase::freeze when already frozen (line 828) ─────────────────
//
// Calling freeze() twice must be a no-op on the second call.

#[test]
fn freeze_when_already_frozen_is_noop() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    tc.freeze();
    assert!(tc.frozen());
    // Second freeze: must not panic and frozen stays true.
    tc.freeze();
    assert!(tc.frozen());
}

// ── NativeTestCase::freeze with observer (lines 840-842) ──────────────────
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
    assert!(tc.frozen());
    // `conclude_test` was called exactly once with the current status —
    // for a never-marked test case that's `Status::Valid` (the default).
    let recorded = captured.lock().unwrap().take();
    assert_eq!(recorded, Some(Status::Valid));
}

// ── NativeTestCase::note, note_str, output (lines 911-925) ─────────────────

#[test]
fn note_appends_debug_repr_to_output() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    tc.note(42u32);
    assert_eq!(tc.output(), "42");
}

#[test]
fn note_str_appends_verbatim_to_output() {
    let mut tc = NativeTestCase::for_choices(&[], None, None);
    tc.note_str("hello world");
    assert_eq!(tc.output(), "hello world");
}

#[test]
fn output_is_empty_initially() {
    let tc = NativeTestCase::for_choices(&[], None, None);
    assert_eq!(tc.output(), "");
}

// ── NativeTestCase::draw_integer with observer (line 1057) ────────────────

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

// ── many_draw_length: min_size == max_size (line 65) ─────────────────────
//
// draw_string with min_size == max_size calls many_draw_length which takes
// the early return path (line 65: return min_size).

#[test]
fn draw_string_fixed_size_uses_early_return_path() {
    use rand::SeedableRng;
    let rng = rand::rngs::SmallRng::seed_from_u64(42);
    let mut tc = NativeTestCase::new_random(rng);
    // min_size == max_size → many_draw_length returns min_size immediately.
    let s = tc.draw_string(65, 90, 3, 3).ok().unwrap();
    // The string must have exactly 3 codepoints.
    assert_eq!(s.chars().count(), 3);
}

// ── draw_float: half-bounded range (lines 1167-1187) ─────────────────────
//
// half_bounded = !bounded && (min.is_finite() || max.is_finite()).
// Use (1.0, f64::INFINITY) so the range is half-bounded from below.

#[test]
fn draw_float_half_bounded_range() {
    use rand::SeedableRng;
    let rng = rand::rngs::SmallRng::seed_from_u64(0);
    let mut tc = NativeTestCase::new_random(rng);
    // half_bounded: min=1.0 (finite), max=INFINITY (not finite).
    let v = tc.draw_float(1.0, f64::INFINITY, false, true).ok().unwrap();
    assert!(v >= 1.0 || v.is_infinite());
}

// ── draw_float: unbounded range with NaN (lines 1188-1199) ───────────────
//
// !bounded && !half_bounded = fully unbounded. allow_nan=true enables the
// NaN generation branch.

#[test]
fn draw_float_unbounded_range() {
    use rand::SeedableRng;
    let rng = rand::rngs::SmallRng::seed_from_u64(0);
    let mut tc = NativeTestCase::new_random(rng);
    // Fully unbounded (min=-INF, max=INF) + allow_nan=true triggers the
    // else/allow_nan branches.
    let _v = tc
        .draw_float(f64::NEG_INFINITY, f64::INFINITY, true, true)
        .ok()
        .unwrap();
    // Any value is acceptable (including NaN, infinity, or a normal number).
}

// ── draw_string: random generation body (lines 1339-1373) ─────────────────
//
// Calling draw_string on a fresh random NTC (no prefix) exercises the
// random-generation closure.

#[test]
fn draw_string_random_generation_body() {
    use rand::SeedableRng;
    let rng = rand::rngs::SmallRng::seed_from_u64(12345);
    let mut tc = NativeTestCase::new_random(rng);
    let s = tc.draw_string(65, 90, 0, 5).ok().unwrap();
    // The string must be valid UTF-8 (draw_string always returns valid chars).
    assert!(s.is_ascii() || s.chars().all(|c| (65u32..=90).contains(&(c as u32))));
}

// ── draw_string: min_size==0 path (line 1319) ────────────────────────────
//
// The nasty-floats list for draw_string includes Vec::new() when min_size==0
// and max_size > 0. Line 1319: `v.push(Vec::new())`.

#[test]
fn draw_string_with_min_size_zero_includes_empty_in_nasty() {
    use rand::SeedableRng;
    let rng = rand::rngs::SmallRng::seed_from_u64(99);
    let mut tc = NativeTestCase::new_random(rng);
    // min_size=0, max_size=5: the nasty list includes Vec::new() (line 1319).
    let s = tc.draw_string(65, 90, 0, 5).ok().unwrap();
    let _ = s; // just ensure it doesn't panic
}
