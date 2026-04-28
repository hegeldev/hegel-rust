#![allow(dead_code)]
// The derive macro generates variables like `basic_Circle` instead of `basic_circle`.
// This is a known issue in the macro; suppress until fixed.
#![allow(non_snake_case)]

mod common;

use common::utils::{
    assert_all_examples, check_can_generate_examples, find_any, one_of_children, schema_type,
};
use hegel::DefaultGenerator as DeriveGenerator;
use hegel::generators::{self as gs, DefaultGenerator, Generator};

// ============================================================================
// Struct definitions
// ============================================================================

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

#[derive(DeriveGenerator, Debug, Clone)]
struct SingleField {
    value: bool,
}

#[derive(DeriveGenerator, Debug, Clone)]
struct ManyFields {
    a: bool,
    b: i32,
    c: String,
    d: u8,
    e: f64,
}

// ============================================================================
// Enum definitions
// ============================================================================

#[derive(DeriveGenerator, Debug, Clone, PartialEq)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(DeriveGenerator, Debug, Clone, PartialEq)]
enum Direction {
    North,
    South,
    East,
    West,
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
enum ComplexEnum {
    Unit,
    Single(bool),
    Named { value: i32 },
    Multi(i32, String),
}

#[derive(DeriveGenerator, Debug, Clone)]
#[allow(clippy::enum_variant_names)]
enum WithNestedTypes {
    VecVariant(Vec<i32>),
    OptionVariant(Option<String>),
    PlainVariant { count: u32 },
}

// ============================================================================
// Basic struct generation tests
// ============================================================================

#[test]
fn test_derive_simple_struct() {
    check_can_generate_examples(gs::default::<Point>());
}

#[test]
fn test_derive_struct_with_multiple_types() {
    check_can_generate_examples(gs::default::<Person>());
}

#[test]
fn test_derive_struct_with_optional_field() {
    check_can_generate_examples(gs::default::<WithOptional>());
}

#[test]
fn test_derive_struct_with_vec_field() {
    check_can_generate_examples(gs::default::<WithVec>());
}

#[test]
fn test_derive_single_field_struct() {
    check_can_generate_examples(gs::default::<SingleField>());
}

#[test]
fn test_derive_many_fields_struct() {
    check_can_generate_examples(gs::default::<ManyFields>());
}

// ============================================================================
// Nested struct generation
// ============================================================================

#[test]
fn test_derive_nested_struct() {
    check_can_generate_examples(gs::default::<WithNested>());
}

// ============================================================================
// Struct field value diversity
// ============================================================================

#[test]
fn test_derive_struct_generates_varied_values() {
    // Verify we get different x values, not always the same thing
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

// ============================================================================
// Struct builder pattern - customizing field generators
// ============================================================================

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
    // Override x but y should still vary
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

// ============================================================================
// Basic enum generation tests
// ============================================================================

#[test]
fn test_derive_unit_enum() {
    check_can_generate_examples(gs::default::<Color>());
}

#[test]
fn test_derive_unit_enum_four_variants() {
    check_can_generate_examples(gs::default::<Direction>());
}

#[test]
fn test_derive_unit_enum_generates_all_variants() {
    find_any(gs::default::<Color>(), |c: &Color| *c == Color::Red);
    find_any(gs::default::<Color>(), |c: &Color| *c == Color::Green);
    find_any(gs::default::<Color>(), |c: &Color| *c == Color::Blue);
}

#[test]
fn test_derive_enum_with_struct_variants() {
    check_can_generate_examples(gs::default::<Shape>());
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
fn test_derive_mixed_enum() {
    check_can_generate_examples(gs::default::<MixedEnum>());
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
fn test_derive_single_variant_data_enum() {
    check_can_generate_examples(gs::default::<SingleVariantData>());
}

#[test]
fn test_derive_tuple_variants_enum() {
    check_can_generate_examples(gs::default::<TupleVariants>());
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
fn test_derive_complex_enum() {
    check_can_generate_examples(gs::default::<ComplexEnum>());
}

#[test]
fn test_derive_complex_enum_generates_all_variants() {
    find_any(gs::default::<ComplexEnum>(), |c: &ComplexEnum| {
        matches!(c, ComplexEnum::Unit)
    });
    find_any(gs::default::<ComplexEnum>(), |c: &ComplexEnum| {
        matches!(c, ComplexEnum::Single(_))
    });
    find_any(gs::default::<ComplexEnum>(), |c: &ComplexEnum| {
        matches!(c, ComplexEnum::Named { .. })
    });
    find_any(gs::default::<ComplexEnum>(), |c: &ComplexEnum| {
        matches!(c, ComplexEnum::Multi(..))
    });
}

#[test]
fn test_derive_enum_with_nested_types() {
    check_can_generate_examples(gs::default::<WithNestedTypes>());
}

// ============================================================================
// Enum builder pattern - customizing variant generators
// ============================================================================

#[test]
fn test_derive_enum_variant_generator_named_fields() {
    // Use per-variant generator directly to constrain fields
    let g = Shape::default_generator().Circle(
        Shape::default_generator()
            .default_Circle()
            .radius(gs::floats().min_value(1.0).max_value(10.0)),
    );
    assert_all_examples(g, |s: &Shape| match s {
        Shape::Circle { radius } => *radius >= 1.0 && *radius <= 10.0,
        Shape::Rectangle { .. } => true,
    });
}

#[test]
fn test_derive_enum_variant_generator_single_tuple() {
    let g = MixedEnum::default_generator().WithValue(
        MixedEnum::default_generator()
            .default_WithValue()
            .value(gs::integers().min_value(0_i32).max_value(100)),
    );
    assert_all_examples(g, |m: &MixedEnum| match m {
        MixedEnum::WithValue(v) => *v >= 0 && *v <= 100,
        _ => true,
    });
}

#[test]
fn test_derive_enum_variant_generator_with_named_fields() {
    let g = MixedEnum::default_generator().WithFields(
        MixedEnum::default_generator()
            .default_WithFields()
            .x(gs::just(99)),
    );
    assert_all_examples(g, |m: &MixedEnum| match m {
        MixedEnum::WithFields { x, .. } => *x == 99,
        _ => true,
    });
}

// ============================================================================
// Derived types used in collections
// ============================================================================

#[test]
fn test_derive_struct_in_vec() {
    check_can_generate_examples(gs::vecs(gs::default::<Point>()));
}

#[test]
fn test_derive_struct_in_option() {
    check_can_generate_examples(gs::optional(gs::default::<Point>()));
}

#[test]
fn test_derive_enum_in_vec() {
    check_can_generate_examples(gs::vecs(gs::default::<Color>()));
}

// ============================================================================
// Derived generators used with combinators
// ============================================================================

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

// ============================================================================
// DefaultGenerator trait integration
// ============================================================================

#[test]
fn test_derive_struct_default_generator_trait() {
    // Verify the DefaultGenerator trait is implemented
    let g = Point::default_generator();
    check_can_generate_examples(g);
}

#[test]
fn test_derive_enum_default_generator_trait() {
    let g = Color::default_generator();
    check_can_generate_examples(g);
}

#[test]
fn test_derive_struct_usable_via_default_function() {
    // gs::default::<T>() should work for derived types
    let g = gs::default::<Point>();
    check_can_generate_examples(g);
}

#[test]
fn test_derive_enum_usable_via_default_function() {
    let g = gs::default::<Color>();
    check_can_generate_examples(g);
}

// ============================================================================
// Using #[hegel::test] with derived types
// ============================================================================

#[hegel::test]
fn test_derive_struct_in_hegel_test(tc: hegel::TestCase) {
    let p: Point = tc.draw(gs::default());
    // Just verify it generates without panicking
    let _ = p.x;
    let _ = p.y;
}

#[hegel::test]
fn test_derive_enum_in_hegel_test(tc: hegel::TestCase) {
    let c: Color = tc.draw(gs::default());
    // Verify it's one of the valid variants
    assert!(matches!(c, Color::Red | Color::Green | Color::Blue));
}

#[hegel::test]
fn test_derive_complex_in_hegel_test(tc: hegel::TestCase) {
    let m: MixedEnum = tc.draw(gs::default());
    match m {
        MixedEnum::Empty => {}
        MixedEnum::WithValue(_v) => {}
        MixedEnum::WithFields { x: _, y: _ } => {}
    }
}

// ============================================================================
// Nested derived types
// ============================================================================

#[hegel::test]
fn test_derive_nested_structs(tc: hegel::TestCase) {
    let w: WithNested = tc.draw(gs::default());
    let _ = w.point.x;
    let _ = w.point.y;
    let _ = w.label;
}

#[test]
fn test_derive_nested_struct_with_custom_inner() {
    let g = WithNested::default_generator()
        .point(Point::default_generator().x(gs::just(0)).y(gs::just(0)));
    assert_all_examples(g, |w: &WithNested| w.point.x == 0 && w.point.y == 0);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_derive_struct_generator_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    // The boxed generator from default() should be Send + Sync
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

// ============================================================================
// Schema shape: enums emit a flat one_of with no tagged-tuple wrapping
// ============================================================================

#[test]
fn test_derive_mixed_enum_emits_flat_one_of() {
    // MixedEnum has Empty (unit), WithValue(i32), WithFields { x, y }.
    let g = MixedEnum::default_generator();
    let basic = g
        .as_basic()
        .expect("MixedEnum default generator should be basic");
    let schema = basic.schema();

    assert_eq!(schema_type(schema), "one_of");

    let children = one_of_children(schema);
    assert_eq!(children.len(), 3);

    // Children appear in declaration order. Crucially, no `[constant(i), child]`
    // wrapping — each child is the variant generator's schema directly:
    //   Empty            -> {"type": "null"}
    //   WithValue(i32)   -> the i32 schema (TupleSingle uses the inner schema)
    //   WithFields {..}  -> {"type": "tuple", ...}
    assert_eq!(schema_type(&children[0]), "null");
    assert_eq!(schema_type(&children[1]), "integer");
    assert_eq!(schema_type(&children[2]), "tuple");
}

#[test]
fn test_derive_complex_enum_emits_flat_one_of_in_declaration_order() {
    // ComplexEnum: Unit, Single(bool), Named { value }, Multi(i32, String).
    let g = ComplexEnum::default_generator();
    let basic = g.as_basic().unwrap();
    let schema = basic.schema();

    assert_eq!(schema_type(schema), "one_of");

    let children = one_of_children(schema);
    assert_eq!(children.len(), 4);
    assert_eq!(schema_type(&children[0]), "null"); // Unit
    // Single(bool): tuple-single uses the inner basic schema directly.
    assert_eq!(schema_type(&children[1]), "boolean");
    assert_eq!(schema_type(&children[2]), "tuple"); // Named
    assert_eq!(schema_type(&children[3]), "tuple"); // Multi
}
