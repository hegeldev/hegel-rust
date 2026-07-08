//! Compile-time properties of `#[derive(hegel::DefaultGenerator)]`.
//!
//! The compile-SUCCESS cases live right here: if this file compiles (and the
//! generic case also runs), the property holds. The compile-FAILURE cases —
//! zero-variant enums, lifetime parameters, a field named `new` — live in
//! `tests/ui/derive_*.rs`, where trybuild pins their diagnostics.
//!
//! IMPORTANT: this file must NOT `use hegel::generators::Generator` (or
//! `hegel::DefaultGenerator`'s companion trait) at module scope — the derive
//! macro's generated code has to compile without the user importing the
//! Generator trait. That is exactly what the first case pins.

mod common;

/// The derive macro's generated code must compile without the user importing
/// the Generator trait. Previously, `new()` called `.boxed()` (a Generator
/// trait method) without importing the trait, so it only compiled when users
/// happened to `use hegel::DefaultGenerator` (which brings both the derive
/// macro AND the trait into scope).
#[derive(Debug, hegel::DefaultGenerator, hegel::PrettyPrintable)]
#[allow(dead_code)]
struct Person {
    name: String,
    age: i32,
}

#[test]
fn test_derive_compiles_without_generator_trait_import() {
    // The property is that `Person` above compiles; drawing one exercises
    // the generated generator end-to-end. (Aliasing the module does not
    // bring the Generator trait into scope.)
    use hegel::generators as gs;

    hegel::Hegel::new(|tc| {
        let p: Person = tc.draw(gs::default::<Person>());
        let _ = (p.name, p.age);
    })
    .settings(hegel::Settings::new().test_cases(5).database(None))
    .run();
}

#[derive(Debug, hegel::DefaultGenerator, hegel::PrettyPrintable)]
struct Point<T> {
    x: T,
    y: i32,
}

#[derive(Debug, hegel::DefaultGenerator, hegel::PrettyPrintable)]
#[allow(dead_code)]
enum Shape<T: std::fmt::Debug> {
    Empty,
    Dot(T),
    Pair { a: T, b: bool },
}

#[derive(Debug, hegel::DefaultGenerator, hegel::PrettyPrintable)]
struct Fixed<const N: usize> {
    xs: [u8; N],
}

#[test]
fn test_derive_on_generic_types_compiles_and_generates() {
    use hegel::generators as gs;

    hegel::Hegel::new(|tc| {
        let p: Point<bool> = tc.draw(gs::default::<Point<bool>>());
        let _ = (p.x, p.y);
        let s: Shape<i32> = tc.draw(gs::default::<Shape<i32>>());
        let _ = format!("{s:?}");
        let f: Fixed<3> = tc.draw(gs::default::<Fixed<3>>());
        assert_eq!(f.xs.len(), 3);
        let q: Point<u8> = tc.draw(gs::default::<Point<u8>>());
        let _ = q;
    })
    .settings(hegel::Settings::new().test_cases(5).database(None))
    .run();
}

/// `Foo` (tuple) generates `foo` and `foo_with` builders; `FooWith` (named)
/// would generate `foo_with` too. Both must fall back to their raw variant
/// idents rather than colliding.
#[derive(Debug, hegel::DefaultGenerator, hegel::PrettyPrintable)]
#[allow(dead_code)]
enum Tricky {
    Foo(u32),
    FooWith { x: u32 },
}

#[test]
fn test_derive_with_variant_with_suffix_collision_compiles() {
    // The property is that `Tricky` above compiles at all.
    let _ = std::any::type_name::<Tricky>();
}
