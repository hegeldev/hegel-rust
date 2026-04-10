#![allow(dead_code)]

mod common;

use common::utils::{assert_all_examples, check_can_generate_examples, find_any};
use hegel::DefaultGenerator as DeriveGenerator;
use hegel::generators::{self as gs, DefaultGenerator, Generator};

#[derive(DeriveGenerator, Debug, Clone)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(DeriveGenerator, Debug, Clone)]
struct Person {
    name: String,
    age: u32,
    active: bool,
}

#[derive(DeriveGenerator, Debug, Clone)]
struct WithOptional {
    label: String,
    value: Option<i32>,
}

#[derive(DeriveGenerator, Debug, Clone)]
struct WithVec {
    items: Vec<i32>,
}

#[derive(DeriveGenerator, Debug, Clone)]
struct WithNested {
    point: Point,
    label: String,
}

#[derive(DeriveGenerator, Debug, Clone, PartialEq)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(DeriveGenerator, Debug, Clone)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

#[derive(DeriveGenerator, Debug, Clone)]
enum MixedEnum {
    Empty,
    WithValue(i32),
    WithFields { x: i32, y: String },
}

#[derive(DeriveGenerator, Debug, Clone)]
enum SingleVariantData {
    Only(String),
}

#[derive(DeriveGenerator, Debug, Clone)]
enum TupleVariants {
    Pair(i32, i32),
    Triple(bool, String, u8),
}

#[derive(DeriveGenerator, Debug, Clone)]
#[allow(clippy::enum_variant_names)]
enum WithNestedTypes {
    VecVariant(Vec<i32>),
    OptionVariant(Option<String>),
    PlainVariant { count: u32 },
}

#[derive(DeriveGenerator, Debug, Clone)]
enum Op {
    Reset,
    Skip,
    Read(usize),
    Seek(i64),
    Write(Vec<u8>),
    ReadWrite(usize, usize),
    Configure { retries: u32, timeout: u64 },
}

#[test]
fn test_derive_struct_with_vec_field() {
    check_can_generate_examples(gs::default::<WithVec>());
    check_can_generate_examples(gs::default::<WithNested>());
    check_can_generate_examples(gs::default::<WithOptional>());
    check_can_generate_examples(gs::default::<Person>());
    check_can_generate_examples(gs::default::<Point>());

    check_can_generate_examples(gs::vecs(gs::default::<Point>()));
    check_can_generate_examples(gs::optional(gs::default::<Point>()));
    check_can_generate_examples(gs::vecs(gs::default::<Color>()));

    check_can_generate_examples(gs::default::<Shape>());
    check_can_generate_examples(gs::default::<MixedEnum>());
    check_can_generate_examples(gs::default::<SingleVariantData>());
    check_can_generate_examples(gs::default::<TupleVariants>());
    check_can_generate_examples(gs::default::<WithNestedTypes>());
}

#[test]
fn test_derive_struct_generates_varied_values() {
    let p1 = find_any(gs::default::<Point>(), |p: &Point| p.x != 0);
    assert_ne!(p1.x, 0);
}

#[test]
fn test_derive_struct_generates_both_bool_values() {
    find_any(gs::default::<Person>(), |p: &Person| p.active);
    find_any(gs::default::<Person>(), |p: &Person| !p.active);
}

#[test]
fn test_derive_struct_with_optional_generates_some_and_none() {
    find_any(gs::default::<WithOptional>(), |w: &WithOptional| {
        w.value.is_some()
    });
    find_any(gs::default::<WithOptional>(), |w: &WithOptional| {
        w.value.is_none()
    });
}

#[test]
fn test_derive_struct_with_custom_field_generator() {
    let g = Point::default_generator().x(gs::just(42));
    assert_all_examples(g, |p: &Point| p.x == 42);
}

#[test]
fn test_derive_struct_with_multiple_custom_fields() {
    let g = Point::default_generator().x(gs::just(1)).y(gs::just(2));
    assert_all_examples(g, |p: &Point| p.x == 1 && p.y == 2);
}

#[test]
fn test_derive_struct_with_constrained_field() {
    let g = Person::default_generator().age(gs::integers().min_value(18_u32).max_value(65));
    assert_all_examples(g, |p: &Person| p.age >= 18 && p.age <= 65);
}

#[test]
fn test_derive_struct_builder_only_overrides_specified_field() {
    let g = Point::default_generator().x(gs::just(0));
    assert_all_examples(g, |p: &Point| p.x == 0);
}

#[test]
fn test_derive_struct_with_mapped_field() {
    let g = Point::default_generator().x(gs::integers::<i32>().map(|x| x.saturating_abs()));
    assert_all_examples(g, |p: &Point| p.x >= 0);
}

#[test]
fn test_derive_struct_with_filtered_field() {
    let g = Point::default_generator().x(gs::integers::<i32>().filter(|x| x % 2 == 0));
    assert_all_examples(g, |p: &Point| p.x % 2 == 0);
}

#[test]
fn test_derive_unit_enum() {
    check_can_generate_examples(gs::default::<Color>());
}

#[test]
fn test_derive_unit_enum_generates_all_variants() {
    find_any(gs::default::<Color>(), |c: &Color| *c == Color::Red);
    find_any(gs::default::<Color>(), |c: &Color| *c == Color::Green);
    find_any(gs::default::<Color>(), |c: &Color| *c == Color::Blue);
}

#[test]
fn test_derive_enum_generates_each_struct_variant() {
    find_any(gs::default::<Shape>(), |s: &Shape| {
        matches!(s, Shape::Circle { .. })
    });
    find_any(gs::default::<Shape>(), |s: &Shape| {
        matches!(s, Shape::Rectangle { .. })
    });
}

#[test]
fn test_derive_mixed_enum_generates_all_variants() {
    find_any(gs::default::<MixedEnum>(), |m: &MixedEnum| {
        matches!(m, MixedEnum::Empty)
    });
    find_any(gs::default::<MixedEnum>(), |m: &MixedEnum| {
        matches!(m, MixedEnum::WithValue(_))
    });
    find_any(gs::default::<MixedEnum>(), |m: &MixedEnum| {
        matches!(m, MixedEnum::WithFields { .. })
    });
}

#[test]
fn test_derive_tuple_variant_generates_both() {
    find_any(gs::default::<TupleVariants>(), |t: &TupleVariants| {
        matches!(t, TupleVariants::Pair(..))
    });
    find_any(gs::default::<TupleVariants>(), |t: &TupleVariants| {
        matches!(t, TupleVariants::Triple(..))
    });
}

#[test]
fn test_derive_enum_variant_generator_named_fields() {
    let g = Shape::default_generator()
        .circle(|g| g.radius(gs::floats().min_value(1.0).max_value(10.0)));
    assert_all_examples(g, |s: &Shape| match s {
        Shape::Circle { radius } => *radius >= 1.0 && *radius <= 10.0,
        Shape::Rectangle { .. } => true,
    });
}

#[test]
fn test_derive_enum_variant_generator_single_tuple() {
    let g =
        MixedEnum::default_generator().with_value(gs::integers().min_value(0_i32).max_value(100));
    assert_all_examples(g, |m: &MixedEnum| match m {
        MixedEnum::WithValue(v) => *v >= 0 && *v <= 100,
        _ => true,
    });
}

#[test]
fn test_derive_enum_variant_generator_multi_tuple() {
    let g = TupleVariants::default_generator().pair(gs::just(42), gs::just(99));
    assert_all_examples(g, |t: &TupleVariants| match t {
        TupleVariants::Pair(a, b) => *a == 42 && *b == 99,
        _ => true,
    });
}

#[test]
fn test_derive_enum_variant_generator_with_named_fields() {
    let g = MixedEnum::default_generator().with_fields(|g| g.x(gs::just(99)));
    assert_all_examples(g, |m: &MixedEnum| match m {
        MixedEnum::WithFields { x, .. } => *x == 99,
        _ => true,
    });
}

#[test]
fn test_derive_mixed_enum_customize_all_variant_kinds() {
    let num_bytes = 1024;
    let g = Op::default_generator()
        .read(gs::integers::<usize>().max_value(num_bytes * 5 / 4))
        .seek(
            gs::integers::<i64>()
                .min_value(-(num_bytes as i64) * 5 / 4)
                .max_value(num_bytes as i64 * 5 / 4),
        )
        .write(gs::vecs(gs::integers::<u8>()).max_size(num_bytes))
        .read_write(
            gs::integers::<usize>().max_value(num_bytes),
            gs::integers::<usize>().max_value(num_bytes),
        )
        .configure(|g| {
            g.retries(gs::integers::<u32>().max_value(3))
                .timeout(gs::integers::<u64>().max_value(1000))
        });
    assert_all_examples(g, move |op: &Op| match op {
        Op::Read(n) => *n <= num_bytes * 5 / 4,
        Op::Seek(offset) => {
            let bound = num_bytes as i64 * 5 / 4;
            *offset >= -bound && *offset <= bound
        }
        Op::Write(data) => data.len() <= num_bytes,
        Op::ReadWrite(r, w) => *r <= num_bytes && *w <= num_bytes,
        Op::Configure { retries, timeout } => *retries <= 3 && *timeout <= 1000,
        Op::Reset | Op::Skip => true,
    });
}

#[test]
fn test_derive_struct_with_map() {
    let g = gs::default::<Point>().map(|p| Point {
        x: p.x.saturating_abs(),
        y: p.y.saturating_abs(),
    });
    assert_all_examples(g, |p: &Point| p.x >= 0 && p.y >= 0);
}

#[test]
fn test_derive_struct_with_filter() {
    let g = gs::default::<Point>().filter(|p| p.x > 0);
    assert_all_examples(g, |p: &Point| p.x > 0);
}

#[test]
fn test_derive_enum_with_filter() {
    let g = gs::default::<Color>().filter(|c| *c != Color::Red);
    assert_all_examples(g, |c: &Color| *c != Color::Red);
}

#[hegel::test]
fn test_derive_struct_in_hegel_test(tc: hegel::TestCase) {
    let _: Point = tc.draw(gs::default());
}

#[hegel::test]
fn test_derive_enum_in_hegel_test(tc: hegel::TestCase) {
    let c: Color = tc.draw(gs::default());
    assert!(matches!(c, Color::Red | Color::Green | Color::Blue));
}

#[hegel::test]
fn test_derive_nested_structs(tc: hegel::TestCase) {
    let _: WithNested = tc.draw(gs::default());
}

#[test]
fn test_derive_nested_struct_with_custom_inner() {
    let g = WithNested::default_generator()
        .point(Point::default_generator().x(gs::just(0)).y(gs::just(0)));
    assert_all_examples(g, |w: &WithNested| w.point.x == 0 && w.point.y == 0);
}

#[test]
fn test_derive_struct_generator_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    let g = gs::default::<Point>();
    assert_send_sync::<hegel::generators::BoxedGenerator<'static, Point>>();
    check_can_generate_examples(g);
}

#[test]
fn test_derive_struct_builder_chaining_order_irrelevant() {
    let g1 = Point::default_generator().x(gs::just(1)).y(gs::just(2));
    let g2 = Point::default_generator().y(gs::just(2)).x(gs::just(1));
    assert_all_examples(g1, |p: &Point| p.x == 1 && p.y == 2);
    assert_all_examples(g2, |p: &Point| p.x == 1 && p.y == 2);
}

#[test]
fn test_derive_struct_override_field_twice_takes_last() {
    let g = Point::default_generator().x(gs::just(1)).x(gs::just(99));
    assert_all_examples(g, |p: &Point| p.x == 99);
}

// FieldName, field_name, and fieldName all convert to "field_name".
// All should keep their original casing.
#[derive(DeriveGenerator, Debug, Clone)]
#[allow(non_camel_case_types)]
enum NameConflict {
    FieldName(i32),
    field_name(String),
    fieldName(bool),
}

#[test]
fn test_derive_enum_triple_conflict() {
    let g = NameConflict::default_generator()
        .FieldName(gs::just(1))
        .field_name(gs::just("hi".to_string()))
        .fieldName(gs::just(true));
    check_can_generate_examples(g);
}
