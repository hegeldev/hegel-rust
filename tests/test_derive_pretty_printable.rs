//! Behaviour of `#[derive(hegel::PrettyPrintable)]`.
//!
//! The compile-FAILURE case — deriving on a union — lives in
//! `tests/ui/derive_pretty_printable_union.rs`, where trybuild pins its
//! diagnostic.

use hegel::{PrettyPrintable, PrettyPrinter};

fn render<T: PrettyPrintable>(value: &T, max_width: usize) -> String {
    let mut printer = PrettyPrinter::new(max_width);
    value.pretty_print(&mut printer);
    printer.value()
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
    assert_eq!(render(&Pair(1, "a".to_string()), 79), "Pair(1, \"a\")");
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
        "Wrapper { value: [1, 2, 3] }"
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
        render(&value, 79),
        "Nested { point: Point { x: 1, y: 2 }, tags: [\"alpha\", \"beta\"] }"
    );
    assert_eq!(
        render(&value, 40),
        "Nested {\n    point: Point { x: 1, y: 2 },\n    tags: [\"alpha\", \"beta\"] }"
    );
    assert_eq!(
        render(&value, 30),
        "Nested {\n    point: Point {\n        x: 1,\n        y: 2 },\n    tags: [\"alpha\", \"beta\"] }"
    );
}

#[derive(PrettyPrintable)]
enum Void {}

#[test]
fn derive_on_an_empty_enum_compiles() {
    // The property is that `Void` above compiles at all.
    let _ = std::any::type_name::<Void>();
}
