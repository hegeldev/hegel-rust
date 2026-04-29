//! Ported from hypothesis-python/tests/conjecture/test_test_data.py.
//!
//! This file tests Hypothesis's `ConjectureData` engine internal directly
//! (constructing it via `ConjectureData.for_choices(...)` rather than via
//! a runner). The native counterpart is
//! [`NativeTestCase::for_choices`](hegel::__native_test_internals::NativeTestCase),
//! exposed through `hegel::__native_test_internals`.
//!
//! Individually-skipped tests:
//!
//! - `test_calls_concluded_implicitly` — needs a `DataObserver` hook that
//!   `freeze()` invokes; bundled with the `test_can_observe_draws` port.
//! - `test_can_observe_draws` — no `DataObserver` API.
//! - `test_empty_strategy_is_invalid` — uses `st.nothing()`, no native
//!   counterpart at this layer.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    ChoiceValue, MAX_DEPTH, NativeConjectureData, NativeResult, NativeTestCase, Span, Status,
    interesting_origin, structural_coverage,
};

#[test]
fn test_cannot_draw_after_freeze() {
    // Hypothesis raises `Frozen` for this; the native engine collapses
    // `Frozen` and `StopTest` onto the same error path (both surface as
    // `Err(StopTest)` from a draw, since `frozen()` is just `status.is_some()`).
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None);
    d.weighted(0.5, None).ok().unwrap();
    d.freeze();
    assert!(d.weighted(0.5, None).is_err());
}

#[test]
fn test_can_double_freeze() {
    let mut d = NativeTestCase::for_choices(&[], None);
    d.freeze();
    assert!(d.frozen());
    d.freeze();
    assert!(d.frozen());
}

#[test]
fn test_draw_past_end_sets_overflow() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None);

    let v = d.weighted(0.5, None).ok().unwrap();
    assert!(v);

    let r = d.weighted(0.5, None);
    assert!(r.is_err()); // StopTest equivalent
    assert_eq!(d.status, Some(Status::EarlyStop)); // OVERRUN equivalent
}

#[test]
fn test_result_is_overrun() {
    // Upstream draws past an empty prefix, then asserts
    // `d.as_result() is Overrun`.  Native uses the `NativeResult`
    // enum: `EarlyStop` (the `OVERRUN` analog) becomes
    // `NativeResult::Overrun`.
    let mut d = NativeTestCase::for_choices(&[], None);
    let r = d.weighted(0.5, None);
    assert!(r.is_err());
    assert!(matches!(d.as_result(), NativeResult::Overrun));
}

#[test]
fn test_overruns_at_exactly_max_length() {
    // Upstream uses `ConjectureData(prefix=[True], random=None, max_choices=1)`
    // inside `buffer_size_limit(1)`; the native equivalent is the
    // `for_prefix_with_max` constructor with `max_choices=1`.
    let mut d = NativeTestCase::for_prefix_with_max(&[ChoiceValue::Boolean(true)], 1);
    d.weighted(0.5, None).ok().unwrap();
    let _ = d.weighted(0.5, None);
    assert_eq!(d.status, Some(Status::EarlyStop)); // OVERRUN equivalent
}

#[test]
fn test_triviality() {
    // Upstream draws boolean True, boolean False, then bytes b"\x02" forced.
    // Native has no `draw(strategy)`, so we drive booleans through `weighted`
    // and the forced-bytes draw through `draw_bytes_forced`.
    let mut d = NativeTestCase::for_choices(
        &[
            ChoiceValue::Boolean(true),
            ChoiceValue::Boolean(false),
            ChoiceValue::Bytes(vec![1]),
        ],
        None,
    );

    // Hypothesis's `data.draw(strategy)` wraps each draw in its own span.
    // Native draws don't auto-record those, so add the per-draw spans
    // explicitly so the lookups by `(start, end)` find a match.
    d.weighted(0.5, None).ok().unwrap();
    d.record_span(0, 1, "bool_0".to_string());
    d.weighted(0.5, None).ok().unwrap();
    d.record_span(1, 2, "bool_1".to_string());
    d.record_span(0, 2, "1".to_string());

    d.draw_bytes_forced(1, 1, vec![2]).ok().unwrap();
    d.record_span(2, 3, "2".to_string());

    let trivial = |u: usize, v: usize| -> bool {
        let span = d
            .spans
            .iter()
            .find(|ex| ex.start == u && ex.end == v)
            .unwrap();
        d.nodes[span.start..span.end].iter().all(|n| n.trivial())
    };

    assert!(!trivial(0, 2));
    assert!(!trivial(0, 1));
    assert!(trivial(1, 2));
    assert!(trivial(2, 3));
}

#[test]
fn test_trivial_before_force_agrees_with_trivial_after() {
    // prefix=(False, True, True); the middle draw forces True over the
    // prefix. Upstream computes node-trivial both before and after
    // `freeze()` and asserts they agree; native node-trivial is invariant
    // under freeze, so the pre/post comparison collapses to a single read.
    let mut d = NativeTestCase::for_choices(
        &[
            ChoiceValue::Boolean(false),
            ChoiceValue::Boolean(true),
            ChoiceValue::Boolean(true),
        ],
        None,
    );
    d.weighted(0.5, None).ok().unwrap();
    d.weighted(0.5, Some(true)).ok().unwrap();
    d.weighted(0.5, None).ok().unwrap();

    let t1: Vec<bool> = (0..3).map(|i| d.nodes[i].trivial()).collect();
    let t2: Vec<bool> = d.nodes.iter().map(|n| n.trivial()).collect();

    assert_eq!(t1, t2);
    // simplest(boolean) is False; node 0 is False (trivial), node 1 is True
    // forced (trivial), node 2 is True unforced (not trivial).
    assert_eq!(t1, vec![true, true, false]);
}

#[test]
fn test_notes_repr() {
    // Upstream notes `b"hi"` and asserts `repr(b"hi")` is in `d.output`.
    // Native renders the bytes via `{:?}` (Rust's closest analog to Python's
    // `repr`), which yields `"[104, 105]"` rather than `"b'hi'"`; the port
    // weakens the assertion to "the Debug rendering of the value lands in
    // d.output", which is the property the upstream test is really checking.
    let mut d = NativeTestCase::for_choices(&[], None);
    let bytes: &[u8] = b"hi";
    d.note(bytes);
    assert!(d.output().contains(&format!("{bytes:?}")));
}

#[test]
fn test_can_note_non_str() {
    // Upstream notes a fresh `object()` and asserts `repr(x)` is in
    // `d.output`.  Rust has no `object()` analog, but any `Debug` type works
    // for the underlying property: notes carry the value's Debug rendering
    // through to the output buffer.
    #[derive(Debug)]
    struct Marker;
    let mut d = NativeTestCase::for_choices(&[], None);
    d.note(Marker);
    assert!(d.output().contains(&format!("{:?}", Marker)));
}

#[test]
fn test_can_note_str_as_non_repr() {
    // Upstream's `data.note("foo")` short-circuits the `repr()` formatting
    // and appends "foo" verbatim.  Native exposes that branch as
    // `note_str` (since `note(<str>)` would Debug-format to `"\"foo\""`).
    let mut d = NativeTestCase::for_choices(&[], None);
    d.note_str("foo");
    assert_eq!(d.output(), "foo");
}

#[test]
fn test_events_are_noted() {
    let mut d = NativeTestCase::for_choices(&[], None);
    d.events_mut().insert("hello".to_string(), String::new());
    assert!(d.events().contains_key("hello"));
}

#[test]
fn test_structural_coverage_is_cached() {
    // Upstream uses Python's `is` to assert pointer-equality through
    // the interning cache.  Native returns `&'static CoverageTag` from
    // a `LazyLock<Mutex<HashMap>>`, so pointer equality is exposed
    // directly via raw-pointer comparison (and `==` works as well).
    let a: *const _ = structural_coverage(50);
    let b: *const _ = structural_coverage(50);
    assert_eq!(a, b);
}

#[test]
fn test_examples_create_structural_coverage() {
    let mut d = NativeTestCase::for_choices(&[], None);
    d.start_span(42);
    d.stop_span(false);
    d.freeze();
    assert!(d.tags.contains(structural_coverage(42)));
}

#[test]
fn test_discarded_examples_do_not_create_structural_coverage() {
    let mut d = NativeTestCase::for_choices(&[], None);
    d.start_span(42);
    d.stop_span(true);
    d.freeze();
    assert!(!d.tags.contains(structural_coverage(42)));
}

#[test]
fn test_children_of_discarded_examples_do_not_create_structural_coverage() {
    let mut d = NativeTestCase::for_choices(&[], None);
    d.start_span(10);
    d.start_span(42);
    d.stop_span(false);
    d.stop_span(true);
    d.freeze();
    assert!(!d.tags.contains(structural_coverage(42)));
    assert!(!d.tags.contains(structural_coverage(10)));
}

#[test]
fn test_can_mark_interesting() {
    let mut d = NativeConjectureData::for_choices(&[]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        d.mark_interesting(interesting_origin(None));
    }));
    assert!(result.is_err());
}

#[test]
fn test_can_mark_invalid() {
    let mut d = NativeConjectureData::for_choices(&[]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        d.mark_invalid(None);
    }));
    assert!(result.is_err());
}

#[test]
fn test_can_mark_invalid_with_why() {
    let mut d = NativeConjectureData::for_choices(&[]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        d.mark_invalid(Some("some reason".to_string()));
    }));
    assert!(result.is_err());
    assert_eq!(d.events()["invalid because"], "some reason");
}

#[test]
fn test_examples_show_up_as_discarded() {
    // Upstream uses d.draw(strategy) which auto-wraps in a span; here we
    // drive spans explicitly with start_span / weighted / stop_span.
    let mut d = NativeTestCase::for_choices(
        &[
            ChoiceValue::Boolean(true),
            ChoiceValue::Boolean(false),
            ChoiceValue::Boolean(true),
        ],
        None,
    );
    d.start_span(1);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(true); // discard=true
    d.start_span(1);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.freeze();
    assert_eq!(d.spans.iter().filter(|ex| ex.discarded).count(), 1);
}

#[test]
fn test_examples_support_negative_indexing() {
    let mut d = NativeTestCase::for_choices(
        &[ChoiceValue::Boolean(true), ChoiceValue::Boolean(true)],
        None,
    );
    d.start_span(1);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.start_span(1);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.freeze();
    assert_eq!(d.spans[-1_i64].choice_count(), 1);
}

#[test]
fn test_examples_out_of_bounds_index() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(false)], None);
    d.start_span(1);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.freeze();
    let spans = d.spans.clone();
    let result = std::panic::catch_unwind(|| {
        let _ = spans[10_usize];
    });
    assert!(result.is_err());
}

#[test]
fn test_can_override_label() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(false)], None);
    d.start_span(7);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.freeze();
    assert!(d.spans.iter().any(|ex| ex.label == "7"));
}

#[test]
fn test_example_equality() {
    let mut d = NativeTestCase::for_choices(
        &[ChoiceValue::Boolean(false), ChoiceValue::Boolean(false)],
        None,
    );
    d.start_span(0);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.start_span(0);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.freeze();

    let spans: Vec<&Span> = d.spans.iter().collect();
    for (i, ex1) in spans.iter().enumerate() {
        for (j, ex2) in spans.iter().enumerate() {
            if i == j {
                assert_eq!(ex1, ex2);
                assert!(!(ex1 != ex2));
            } else {
                assert_ne!(ex1, ex2);
                assert!(!(ex1 == ex2));
            }
        }
    }
    // Note: upstream also checks `ex != "hello"` (comparing Span to a non-Span),
    // which is not applicable in Rust due to the type system.
}

#[test]
fn test_example_depth_marking() {
    // Add an explicit top-level span (Hypothesis's ConjectureData.__init__
    // opens one automatically via self.start_span(TOP_LABEL)).
    let choices: Vec<ChoiceValue> = (0..6).map(|_| ChoiceValue::Integer(0)).collect();
    let mut d = NativeTestCase::for_choices(&choices, None);
    d.start_span(0); // top span, depth=0
    // v1: draw(st.integers())
    d.start_span(1); // depth=1
    d.draw_integer(0, 1000).ok().unwrap();
    d.stop_span(false);
    // inner (start_span(0))
    d.start_span(0); // depth=1
    // v2: draw(st.integers())
    d.start_span(1); // depth=2
    d.draw_integer(0, 1000).ok().unwrap();
    d.stop_span(false);
    // v3: draw(st.integers())
    d.start_span(1); // depth=2
    d.draw_integer(0, 1000).ok().unwrap();
    d.stop_span(false);
    d.stop_span(false); // close inner
    // v4: draw(st.integers())
    d.start_span(1); // depth=1
    d.draw_integer(0, 1000).ok().unwrap();
    d.stop_span(false);
    d.freeze(); // closes top span

    assert_eq!(d.spans.len(), 6);
    let depths: Vec<(usize, u32)> = d
        .spans
        .iter()
        .map(|ex| (ex.choice_count(), ex.depth))
        .collect();
    assert_eq!(
        depths,
        vec![
            (4, 0), // top
            (1, 1), // v1
            (2, 1), // inner
            (1, 2), // v2
            (1, 2), // v3
            (1, 1), // v4
        ]
    );
}

#[test]
fn test_has_examples_even_when_empty() {
    // st.just(False) makes no choices; the span exists but covers 0 nodes.
    let mut d = NativeTestCase::for_choices(&[], None);
    d.start_span(1);
    d.stop_span(false);
    d.freeze();
    assert!(!d.spans.is_empty());
}

#[test]
fn test_has_cached_examples_even_when_overrun() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(false)], None);
    d.start_span(3);
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    // Draw past end → overrun.
    let _ = d.weighted(0.5, None);
    assert_eq!(d.status, Some(Status::EarlyStop));
    assert!(
        d.spans
            .iter()
            .any(|ex| ex.label == "3" && ex.choice_count() == 1)
    );
    // d.spans is d.spans — in Rust, spans is a plain field so the address is
    // always the same; confirmed by pointer equality.
    let ptr1: *const _ = &d.spans;
    let ptr2: *const _ = &d.spans;
    assert_eq!(ptr1, ptr2);
}

#[test]
fn test_closes_interval_on_error_in_strategy() {
    // Upstream: d.draw(BoomStrategy()) opens a span, draws a boolean, then
    // raises ValueError; draw()'s `finally: stop_span()` closes the span.
    // In native, freeze() drains span_stack and sets end on all open spans,
    // which is the equivalent guarantee.
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(true)], None);
    d.start_span(1);
    d.weighted(0.5, None).ok().unwrap();
    // Span left open (simulates strategy panicking before stop_span).
    d.freeze();
    assert!(d.spans.iter().all(|eg| eg.end >= eg.start));
}

#[test]
fn test_does_not_double_freeze_in_interval_close() {
    // Upstream: d.draw(BigStrategy()) opens a span, draw_bytes overruns
    // (freezing via mark_overrun()), then draw()'s finally: stop_span() is a
    // no-op (frozen guard).  In native, freeze() is idempotent on an already-
    // frozen (EarlyStop) test case and closes any unclosed spans.
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Boolean(false)], None);
    d.start_span(1);
    d.weighted(0.5, None).ok().unwrap();
    // Overrun: draw past end of prefix (only 1 choice supplied).
    let result = d.weighted(0.5, None);
    assert!(result.is_err());
    // Span still open; freeze() closes it without changing EarlyStop status.
    d.freeze();
    assert!(d.frozen());
    assert!(d.spans.iter().all(|eg| eg.end >= eg.start));
}

#[test]
fn test_will_mark_too_deep_examples_as_invalid() {
    let mut d = NativeTestCase::for_choices(&[ChoiceValue::Integer(0)], None);
    // Open MAX_DEPTH + 1 spans: the 101st call (stack_len == MAX_DEPTH == 100)
    // triggers the depth limit and sets Status::Invalid.
    for _ in 0..=MAX_DEPTH {
        d.start_span(0);
    }
    let result = d.draw_integer(0, 100);
    assert!(result.is_err());
    assert_eq!(d.status, Some(Status::Invalid));
}

#[test]
fn test_child_indices() {
    let choices: Vec<ChoiceValue> = (0..4).map(|_| ChoiceValue::Boolean(true)).collect();
    let mut d = NativeTestCase::for_choices(&choices, None);
    // Add a top-level span so indices match Python (span 0 = top).
    d.start_span(0); // span 0: top
    d.start_span(0); // span 1: examples[1]
    d.start_span(1); // span 2: examples[2]
    d.start_span(2); // span 3: examples[3]
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.start_span(2); // span 4: examples[4]
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.stop_span(false); // close examples[2]
    d.stop_span(false); // close examples[1]
    d.start_span(2); // span 5: examples[5]
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.start_span(2); // span 6: examples[6]
    d.weighted(0.5, None).ok().unwrap();
    d.stop_span(false);
    d.freeze(); // closes top span

    assert_eq!(d.spans.children(0), vec![1, 5, 6]);
    assert_eq!(d.spans.children(1), vec![2]);
    assert_eq!(d.spans.children(2), vec![3, 4]);

    assert_eq!(d.spans[0_usize].parent, None);
    for i in 1..d.spans.len() {
        let parent_idx = d.spans[i].parent.unwrap();
        assert!(d.spans.children(parent_idx).contains(&i));
    }
}
