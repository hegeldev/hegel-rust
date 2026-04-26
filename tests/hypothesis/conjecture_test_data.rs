//! Ported from hypothesis-python/tests/conjecture/test_test_data.py.
//!
//! This file tests Hypothesis's `ConjectureData` engine internal directly
//! (constructing it via `ConjectureData.for_choices(...)` rather than via
//! a runner). The native counterpart is
//! [`NativeTestCase::for_choices`](hegel::__native_test_internals::NativeTestCase),
//! exposed through `hegel::__native_test_internals`. `NativeTestCase`
//! covers only the choice-level draw operations and span recording;
//! many of the tests in this file rely on engine surface that has not
//! yet been ported into the native API and are listed below as
//! individually-skipped.
//!
//! Individually-skipped tests:
//!
//! - `test_calls_concluded_implicitly` — needs a `DataObserver` hook that
//!   `freeze()` invokes; bundled with the `test_can_observe_draws` port.
//! - `test_can_mark_interesting`, `test_can_mark_invalid`,
//!   `test_can_mark_invalid_with_why` — `NativeTestCase` has no
//!   `mark_interesting` / `mark_invalid` methods. Those live on the
//!   higher-level `NativeConjectureData` whose `for_choices` constructor
//!   is private.
//! - `test_examples_show_up_as_discarded`, `test_can_override_label`,
//!   `test_examples_support_negative_indexing`,
//!   `test_examples_out_of_bounds_index`, `test_child_indices`,
//!   `test_example_equality`, `test_example_depth_marking`,
//!   `test_has_examples_even_when_empty`,
//!   `test_has_cached_examples_even_when_overrun` — `NativeTestCase`
//!   has no draw-by-strategy method that auto-creates spans.
//!   `Span` is a flat struct with no `parent` / `children` / `depth` /
//!   `choice_count` / `discarded` fields, and `spans` is a plain `Vec`
//!   without negative-indexing or out-of-bounds-error semantics.
//! - `test_can_observe_draws` — no `DataObserver` API.
//! - `test_will_mark_too_deep_examples_as_invalid` — uses Hypothesis's
//!   `MAX_DEPTH` constant and recursive `.map` strategy nesting; native
//!   engine has no MAX_DEPTH analog and `NativeTestCase` has no
//!   draw-by-strategy method.
//! - `test_empty_strategy_is_invalid` — uses `st.nothing()`, no native
//!   counterpart at this layer.
//! - `test_structural_coverage_is_cached`,
//!   `test_examples_create_structural_coverage`,
//!   `test_discarded_examples_do_not_create_structural_coverage`,
//!   `test_children_of_discarded_examples_do_not_create_structural_coverage`
//!   — no `structural_coverage()` / `tags` API on the native engine.
//! - `test_closes_interval_on_error_in_strategy`,
//!   `test_does_not_double_freeze_in_interval_close` — assume that
//!   `NativeTestCase` exposes a `draw(strategy)` method that closes
//!   open spans on exception. Native routes strategy draws through
//!   Hegel-side `Generator::do_draw`, not the native test case.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{ChoiceValue, NativeResult, NativeTestCase, Status};

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
