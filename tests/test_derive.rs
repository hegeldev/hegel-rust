#![allow(dead_code)]

mod common;

use common::utils::{assert_all_examples, check_can_generate_examples, find_any};
use hegel::DefaultGenerator as DeriveGenerator;
use hegel::generators::{self as gs, DefaultGenerator, Generator};

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
struct Person {
    name: String,
    age: u32,
    active: bool,
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
struct WithOptional {
    label: String,
    value: Option<i32>,
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
struct WithVec {
    items: Vec<i32>,
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
struct WithNested {
    point: Point,
    label: String,
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone, PartialEq)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
enum MixedEnum {
    Empty,
    WithValue(i32),
    WithFields { x: i32, y: String },
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
enum SingleVariantData {
    Only(String),
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
enum TupleVariants {
    Pair(i32, i32),
    Triple(bool, String, u8),
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
#[allow(clippy::enum_variant_names)]
enum WithNestedTypes {
    VecVariant(Vec<i32>),
    OptionVariant(Option<String>),
    PlainVariant { count: u32 },
}

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
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
fn test_default_supports_struct_builder() {
    let g = gs::default::<Point>().x(gs::just(42));
    assert_all_examples(g, |p: &Point| p.x == 42);
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
fn test_default_supports_enum_variant_builder() {
    let g =
        gs::default::<Shape>().circle(|g| g.radius(gs::floats().min_value(1.0).max_value(10.0)));
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
fn test_derive_enum_variant_named_dependent_via_compose() {
    let g = Op::default_generator().configure(|_g| {
        hegel::compose!(|tc| {
            let retries = tc.draw(gs::integers::<u32>().min_value(1).max_value(10));
            Op::Configure {
                retries,
                timeout: u64::from(retries) * 100,
            }
        })
    });
    assert_all_examples(g, |op: &Op| match op {
        Op::Configure { retries, timeout } => {
            *retries >= 1 && *retries <= 10 && u64::from(*retries) * 100 == *timeout
        }
        _ => true,
    });
}

#[test]
fn test_derive_enum_variant_multi_tuple_with_compose() {
    let g = Op::default_generator().read_write_with(|_g| {
        hegel::compose!(|tc| {
            let n = tc.draw(gs::integers::<usize>().min_value(1).max_value(100));
            Op::ReadWrite(n, n)
        })
    });
    assert_all_examples(g, |op: &Op| match op {
        Op::ReadWrite(r, w) => r == w && *r >= 1 && *r <= 100,
        _ => true,
    });
}

#[test]
fn test_derive_enum_variant_multi_tuple_with_partial() {
    let g = Op::default_generator().read_write_with(|g| g._1(gs::just(42)));
    assert_all_examples(g, |op: &Op| match op {
        Op::ReadWrite(_, w) => *w == 42,
        _ => true,
    });
}

#[test]
fn test_derive_enum_variant_single_tuple_with() {
    let g = Op::default_generator().read_with(|g| g._0(gs::just(7)));
    assert_all_examples(g, |op: &Op| match op {
        Op::Read(n) => *n == 7,
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
    let _ = tc.draw(gs::default::<Point>());
}

#[hegel::test]
fn test_derive_enum_in_hegel_test(tc: hegel::TestCase) {
    let c = tc.draw(gs::default::<Color>());
    assert!(matches!(c, Color::Red | Color::Green | Color::Blue));
}

#[hegel::test]
fn test_derive_nested_structs(tc: hegel::TestCase) {
    let _ = tc.draw(gs::default::<WithNested>());
}

#[test]
fn test_derive_nested_struct_with_custom_inner() {
    let g = WithNested::default_generator()
        .point(Point::default_generator().x(gs::just(0)).y(gs::just(0)));
    assert_all_examples(g, |w: &WithNested| w.point.x == 0 && w.point.y == 0);
}

#[test]
fn test_derive_struct_generator_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    let g = gs::default::<Point>();
    assert_send_sync(&g);
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

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
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

#[derive(DeriveGenerator, hegel::PrettyPrintable, Debug, Clone)]
#[allow(non_camel_case_types)]
enum KeywordVariants {
    Super(i32),
    Type(u32),
    r#type(u64),
    r#Crate(bool),
}

#[test]
fn test_derive_enum_with_keyword_variants() {
    let g = KeywordVariants::default_generator()
        .super_(gs::just(-1_i32))
        .type_(gs::just(7_u32))
        .r#type(gs::just(99_u64))
        .r#crate_(gs::just(true));
    assert_all_examples(g, |v: &KeywordVariants| match v {
        KeywordVariants::Super(n) => *n == -1,
        KeywordVariants::Type(n) => *n == 7,
        KeywordVariants::r#type(n) => *n == 99,
        KeywordVariants::r#Crate(b) => *b,
    });
}

/// A deliberately non-printable generator: field builders accept any
/// [`Generator`], and printability of the derived generator is decided by
/// its current field generators.
struct SilentSmallInt;

impl Generator<i32> for SilentSmallInt {
    fn do_draw(&self, tc: &hegel::TestCase) -> i32 {
        tc.draw_silent(gs::integers::<i32>().min_value(1).max_value(3))
    }
}

#[test]
fn test_derive_struct_field_accepts_a_non_printable_generator() {
    hegel::Hegel::new(|tc| {
        let p: Point = tc.draw_silent(Point::default_generator().x(SilentSmallInt));
        assert!((1..=3).contains(&p.x));
    })
    .settings(hegel::Settings::new().database(None))
    .run();
}

#[test]
fn test_derive_struct_field_print_with_restores_printability() {
    hegel::Hegel::new(|tc| {
        let p: Point = tc.draw(
            Point::default_generator()
                .x(SilentSmallInt.print_with(|v, printer| printer.text(&format!("{v}")))),
        );
        assert!((1..=3).contains(&p.x));
    })
    .settings(hegel::Settings::new().database(None))
    .run();
}

/// A type whose hand-written `DefaultGenerator` produces a non-printable
/// generator: deriving `DefaultGenerator` on a struct containing it still
/// compiles, and the derived generator is drawable silently.
#[derive(Debug, Clone)]
struct Opaque(i32);

struct OpaqueGenerator;

impl Generator<Opaque> for OpaqueGenerator {
    fn do_draw(&self, tc: &hegel::TestCase) -> Opaque {
        Opaque(tc.draw_silent(gs::integers::<i32>()))
    }
}

impl DefaultGenerator for Opaque {
    type Generator = OpaqueGenerator;
    fn default_generator() -> Self::Generator {
        OpaqueGenerator
    }
}

#[derive(DeriveGenerator, Debug, Clone)]
struct HasOpaque {
    id: u32,
    payload: Opaque,
}

#[test]
fn test_derive_with_non_printable_default_field_generator_draws_silently() {
    hegel::Hegel::new(|tc| {
        let v: HasOpaque = tc.draw_silent(gs::default::<HasOpaque>());
        let _ = v.payload.0;
    })
    .settings(hegel::Settings::new().database(None))
    .run();
}

#[test]
fn test_derive_with_non_printable_default_becomes_printable_via_builder() {
    hegel::Hegel::new(|tc| {
        let v: HasOpaque = tc.draw(gs::default::<HasOpaque>().payload(
            OpaqueGenerator.print_with(|v, printer| printer.text(&format!("Opaque({})", v.0))),
        ));
        let _ = v.id;
    })
    .settings(hegel::Settings::new().database(None))
    .run();
}

struct SilentSmallFloat;

impl Generator<f64> for SilentSmallFloat {
    fn do_draw(&self, tc: &hegel::TestCase) -> f64 {
        tc.draw_silent(gs::floats::<f64>().min_value(0.0).max_value(1.0))
    }
}

#[test]
fn test_derive_enum_variant_builder_accepts_plain_generators() {
    hegel::Hegel::new(|tc| {
        let s: Shape =
            tc.draw_silent(Shape::default_generator().circle(|g| g.radius(SilentSmallFloat)));
        if let Shape::Circle { radius } = s {
            assert!((0.0..=1.0).contains(&radius));
        }
    })
    .settings(hegel::Settings::new().database(None))
    .run();
}
