mod common;

use common::project::TempRustProject;

/// The derive macro's generated code must compile without the user importing
/// the Generator trait. Previously, `new()` called `.boxed()` (a Generator
/// trait method) without importing the trait, so it only compiled when users
/// happened to `use hegel::DefaultGenerator` (which brings both the derive
/// macro AND the trait into scope).
#[test]
fn test_derive_compiles_without_generator_trait_import() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(Debug, hegel::DefaultGenerator)]
struct Person {
    name: String,
    age: i32,
}

fn main() {}
"#,
        )
        .cargo_run(&[]);
}

#[test]
fn test_derive_on_zero_variant_enum_is_a_clean_compile_error() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(Debug, hegel::DefaultGenerator)]
enum Void {}

fn main() {}
"#,
        )
        .expect_failure("cannot be derived for enums with no variants")
        .cargo_run(&[]);
}

#[test]
fn test_derive_on_generic_types_compiles_and_generates() {
    TempRustProject::new()
        .main_file(
            r#"
use hegel::generators as gs;

#[derive(Debug, hegel::DefaultGenerator)]
struct Point<T> {
    x: T,
    y: i32,
}

#[derive(Debug, hegel::DefaultGenerator)]
#[allow(dead_code)]
enum Shape<T: std::fmt::Debug> {
    Empty,
    Dot(T),
    Pair { a: T, b: bool },
}

#[derive(Debug, hegel::DefaultGenerator)]
struct Fixed<const N: usize> {
    xs: [u8; N],
}

fn main() {
    hegel::Hegel::new(|tc| {
        let p: Point<bool> = tc.draw(gs::default::<Point<bool>>());
        let _ = (p.x, p.y);
        let s: Shape<i32> = tc.draw(gs::default::<Shape<i32>>());
        let _ = format!("{s:?}");
        let f: Fixed<3> = tc.draw(gs::default::<Fixed<3>>());
        assert_eq!(f.xs.len(), 3);
        let q: Point<u8> = tc.draw(
            gs::default::<Point<u8>>(),
        );
        let _ = q;
    })
    .settings(hegel::Settings::new().test_cases(5).database(None))
    .run();
}
"#,
        )
        .cargo_run(&[]);
}

#[test]
fn test_derive_on_lifetime_generic_type_is_a_clean_compile_error() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(Debug, hegel::DefaultGenerator)]
struct Borrowed<'a> {
    x: &'a str,
}

fn main() {}
"#,
        )
        .expect_failure("does not support lifetime parameters")
        .cargo_run(&[]);
}

#[test]
fn test_derive_with_field_named_new_is_a_clean_compile_error() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(Debug, hegel::DefaultGenerator)]
struct Odd {
    new: bool,
}

fn main() {}
"#,
        )
        .expect_failure("collides with the generated builder API")
        .cargo_run(&[]);
}

/// `Foo` (tuple) generates `foo` and `foo_with` builders; `FooWith` (named)
/// would generate `foo_with` too. Both must fall back to their raw variant
/// idents rather than colliding.
#[test]
fn test_derive_with_variant_with_suffix_collision_compiles() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(Debug, hegel::DefaultGenerator)]
#[allow(dead_code)]
enum Tricky {
    Foo(u32),
    FooWith { x: u32 },
}

fn main() {}
"#,
        )
        .cargo_run(&[]);
}
