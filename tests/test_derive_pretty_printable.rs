//! Behaviour of `#[derive(hegel::PrettyPrintable)]`.
//!
//! The compile-FAILURE case — deriving on a union — lives in
//! `tests/ui/derive_pretty_printable_union.rs`, where trybuild pins its
//! diagnostic.

use hegel::{Document, PrettyPrintable};

fn render<T: PrettyPrintable>(value: &T, max_width: usize) -> String {
    let mut doc = Document::new().max_width(max_width);
    value.pretty_print(doc.printer());
    doc.finish()
}

#[derive(PrettyPrintable)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn named_struct_prints_inline_when_it_fits() {
    assert_eq!(render(&Point { x: 1, y: 2 }, 79), "Point { x: 1, y: 2 }");
}

#[test]
fn named_struct_breaks_one_field_per_line() {
    assert_eq!(
        render(&Point { x: 1, y: 2 }, 12),
        "Point {\n    x: 1,\n    y: 2 }"
    );
}

#[derive(PrettyPrintable)]
struct Pair(i32, String);

#[test]
fn tuple_struct_prints_like_a_call() {
    assert_eq!(
        render(&Pair(1, "a".to_string()), 79),
        "Pair(1, \"a\".to_string())"
    );
}

#[derive(PrettyPrintable)]
struct Marker;

#[derive(PrettyPrintable)]
struct EmptyBraced {}

#[derive(PrettyPrintable)]
struct EmptyTuple();

#[test]
fn fieldless_structs_print_as_expressions() {
    assert_eq!(render(&Marker, 79), "Marker");
    assert_eq!(render(&EmptyBraced {}, 79), "EmptyBraced {}");
    assert_eq!(render(&EmptyTuple(), 79), "EmptyTuple()");
}

#[derive(PrettyPrintable)]
enum Shape {
    Empty,
    Circle(u32),
    Rect { width: u32, height: u32 },
}

#[test]
fn enum_variants_print_qualified() {
    assert_eq!(render(&Shape::Empty, 79), "Shape::Empty");
    assert_eq!(render(&Shape::Circle(5), 79), "Shape::Circle(5)");
    assert_eq!(
        render(
            &Shape::Rect {
                width: 1,
                height: 2
            },
            79
        ),
        "Shape::Rect { width: 1, height: 2 }"
    );
}

#[derive(PrettyPrintable)]
struct Wrapper<T> {
    value: T,
}

#[derive(PrettyPrintable)]
enum Maybe<T> {
    Nothing,
    Just(T),
}

#[test]
fn generic_types_get_pretty_printable_bounds() {
    assert_eq!(render(&Wrapper { value: 5 }, 79), "Wrapper { value: 5 }");
    assert_eq!(
        render(
            &Wrapper {
                value: vec![1, 2, 3]
            },
            79
        ),
        "Wrapper { value: vec![1, 2, 3] }"
    );
    assert_eq!(render(&Maybe::<i32>::Nothing, 79), "Maybe::Nothing");
    assert_eq!(render(&Maybe::Just("x"), 79), "Maybe::Just(\"x\")");
}

#[derive(PrettyPrintable)]
struct Borrowed<'a> {
    name: &'a str,
}

#[derive(PrettyPrintable)]
struct Fixed<const N: usize> {
    data: [u8; N],
}

#[test]
fn lifetimes_and_const_generics_are_supported() {
    assert_eq!(
        render(&Borrowed { name: "hi" }, 79),
        "Borrowed { name: \"hi\" }"
    );
    assert_eq!(
        render(&Fixed::<2> { data: [1, 2] }, 79),
        "Fixed { data: [1, 2] }"
    );
}

#[derive(PrettyPrintable)]
struct Nested {
    point: Point,
    tags: Vec<String>,
}

#[test]
fn nested_derives_compose_and_wrap() {
    let value = Nested {
        point: Point { x: 1, y: 2 },
        tags: vec!["alpha".to_string(), "beta".to_string()],
    };
    assert_eq!(
        render(&value, 100),
        "Nested { point: Point { x: 1, y: 2 }, tags: vec![\"alpha\".to_string(), \"beta\".to_string()] }"
    );
    assert_eq!(
        render(&value, 60),
        "Nested {\n    point: Point { x: 1, y: 2 },\n    tags: vec![\"alpha\".to_string(), \"beta\".to_string()] }"
    );
    assert_eq!(
        render(&value, 30),
        "Nested {\n    point: Point {\n        x: 1,\n        y: 2 },\n    tags: vec![\"alpha\".to_string(),\n         \"beta\".to_string()] }"
    );
}

#[derive(PrettyPrintable)]
enum Void {}

#[test]
fn derive_on_an_empty_enum_compiles() {
    // The property is that `Void` above compiles at all.
    let _ = std::any::type_name::<Void>();
}

#[derive(PrettyPrintable)]
struct WithForeign {
    name: String,
    #[pretty(debug)]
    path: std::path::PathBuf,
}

#[test]
fn pretty_debug_fields_print_their_debug_representation() {
    let value = WithForeign {
        name: "a".to_string(),
        path: std::path::PathBuf::from("/tmp"),
    };
    assert_eq!(
        render(&value, 79),
        "WithForeign { name: \"a\".to_string(), path: \"/tmp\" }"
    );
}

#[derive(Debug)]
struct DebugInner {
    #[allow(dead_code)]
    items: Vec<i32>,
}

#[derive(PrettyPrintable)]
struct DebugOuter {
    #[pretty(debug)]
    inner: DebugInner,
}

#[test]
fn pretty_debug_fields_relayout_derived_debug_output() {
    let value = DebugOuter {
        inner: DebugInner {
            items: vec![1000, 2000, 3000],
        },
    };
    assert_eq!(
        render(&value, 79),
        "DebugOuter { inner: DebugInner { items: [1000, 2000, 3000] } }"
    );
    assert_eq!(
        render(&value, 24),
        "DebugOuter {\n    inner: DebugInner {\n        items: [1000,\n         2000,\n         3000] } }"
    );
}

#[derive(PrettyPrintable)]
enum ForeignMessage {
    Payload {
        #[pretty(debug)]
        data: std::ffi::OsString,
    },
    Raw(#[pretty(debug)] std::ffi::OsString),
}

#[test]
fn pretty_debug_fields_work_in_enum_variants() {
    assert_eq!(
        render(
            &ForeignMessage::Payload {
                data: std::ffi::OsString::from("x")
            },
            79
        ),
        "ForeignMessage::Payload { data: \"x\" }"
    );
    assert_eq!(
        render(&ForeignMessage::Raw(std::ffi::OsString::from("y")), 79),
        "ForeignMessage::Raw(\"y\")"
    );
}

#[derive(PrettyPrintable)]
struct GenericDebugField<T> {
    #[pretty(debug)]
    raw: T,
}

#[test]
fn pretty_debug_fields_add_a_debug_bound_for_generic_fields() {
    assert_eq!(
        render(&GenericDebugField { raw: 5 }, 79),
        "GenericDebugField { raw: 5 }"
    );
}
