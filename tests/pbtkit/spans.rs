//! Ported from resources/pbtkit/tests/test_spans.py

#![cfg(feature = "native")]

use std::collections::HashSet;

use crate::common::utils::find_any;
use hegel::__native_test_internals::{CachedTestFunction, ChoiceValue, NativeTestCase};
use hegel::TestCase;
use hegel::generators::{self as gs};

#[test]
fn test_draw_records_spans() {
    // Each draw() call creates a span covering the choices it used.
    let mut ctf = CachedTestFunction::new(|tc: TestCase| {
        tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
        tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
    });
    let choices = vec![ChoiceValue::Integer(3), ChoiceValue::Integer(5)];
    let ntc = NativeTestCase::for_choices(&choices, None, None);
    let (_, nodes, spans) = ctf.run(ntc);

    assert_eq!(nodes.len(), 2);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, 1);
    assert_eq!(spans[1].start, 1);
    assert_eq!(spans[1].end, 2);
    assert!(
        spans[0].label.contains("integer"),
        "expected 'integer' in label, got '{}'",
        spans[0].label
    );
    assert!(
        spans[1].label.contains("integer"),
        "expected 'integer' in label, got '{}'",
        spans[1].label
    );
}

#[test]
fn test_nested_spans() {
    // Composite generators (tuples) create leaf spans for each element draw.
    // Unlike Python's pbtkit, Rust does not record an outer composite span
    // (start_span/stop_span are no-ops in the native backend).
    let mut ctf = CachedTestFunction::new(|tc: TestCase| {
        tc.draw(gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(5),
            gs::integers::<i64>().min_value(0).max_value(5),
        ));
    });
    let choices = vec![ChoiceValue::Integer(1), ChoiceValue::Integer(2)];
    let ntc = NativeTestCase::for_choices(&choices, None, None);
    let (_, nodes, spans) = ctf.run(ntc);

    assert_eq!(nodes.len(), 2);
    // Rust records only leaf (integer) spans; no outer tuple span.
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, 1); // first integer
    assert_eq!(spans[1].start, 1);
    assert_eq!(spans[1].end, 2); // second integer
}

#[test]
fn test_list_draw_has_spans() {
    // Drawing a list creates spans for its element draws.
    // Unlike Python, Rust does not record an outer list span; only the
    // integer leaf draws are tracked. Collection-control booleans (many_more)
    // are drawn via ntc.weighted(), which bypasses interpret_schema and
    // therefore does not produce a span.
    let mut ctf = CachedTestFunction::new(|tc: TestCase| {
        tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(10)).max_size(5));
    });
    // Boolean(true)=continue, Integer(3)=element value, Boolean(false)=stop.
    let choices = vec![
        ChoiceValue::Boolean(true),
        ChoiceValue::Integer(3),
        ChoiceValue::Boolean(false),
    ];
    let ntc = NativeTestCase::for_choices(&choices, None, None);
    let (_, nodes, spans) = ctf.run(ntc);

    assert_eq!(nodes.len(), 3);
    assert_eq!(spans.len(), 1);
    // The integer element is at node index 1 (after the first continue-boolean).
    assert_eq!(spans[0].start, 1);
    assert_eq!(spans[0].end, 2);
    assert!(
        spans[0].label.contains("integer"),
        "expected 'integer' in label, got '{}'",
        spans[0].label
    );
}

#[test]
fn test_span_mutation_finds_duplicate() {
    // Span mutation can find duplicate compound elements in a list.
    // The native runner uses try_span_mutation to efficiently discover
    // lists of tuples that contain repeated entries.
    let result = find_any(
        gs::vecs(gs::tuples!(
            gs::integers::<i64>().min_value(0).max_value(100),
            gs::integers::<i64>().min_value(0).max_value(100),
        ))
        .max_size(10),
        |ls: &Vec<(i64, i64)>| {
            let unique: HashSet<_> = ls.iter().collect();
            ls.len() != unique.len()
        },
    );
    assert!(!result.is_empty());
}
