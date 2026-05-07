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
