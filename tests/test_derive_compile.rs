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
fn test_hegel_test_with_empty_parens() {
    TempRustProject::new()
        .test_file(
            "empty_parens.rs",
            r#"
#[hegel::test()]
fn my_test(tc: hegel::TestCase) {
    let _ = tc.draw(hegel::generators::booleans());
}
"#,
        )
        .main_file("fn main() {}")
        .cargo_test(&[]);
}

#[test]
fn test_hegel_test_on_non_function() {
    TempRustProject::new()
        .test_file(
            "not_fn.rs",
            r#"
#[hegel::test]
struct NotAFunction {
    x: i32,
}
"#,
        )
        .main_file("fn main() {}")
        .expect_failure("expected")
        .cargo_test(&[]);
}

#[test]
fn test_hegel_test_rejects_invalid_attribute_args() {
    TempRustProject::new()
        .test_file(
            "bad_attr.rs",
            r#"
#[hegel::test(not a = valid expression here)]
fn my_test(tc: hegel::TestCase) {
    let _ = tc.draw(hegel::generators::booleans());
}
"#,
        )
        .main_file("fn main() {}")
        .expect_failure("expected")
        .cargo_test(&[]);
}

#[test]
fn test_derive_rejects_tuple_struct() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(Debug, hegel::DefaultGenerator)]
struct Pair(i32, i32);

fn main() {}
"#,
        )
        .expect_failure("named fields")
        .cargo_run(&[]);
}

#[test]
fn test_derive_rejects_unit_struct() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(Debug, hegel::DefaultGenerator)]
struct Marker;

fn main() {}
"#,
        )
        .expect_failure("unit structs")
        .cargo_run(&[]);
}

#[test]
fn test_derive_rejects_union() {
    TempRustProject::new()
        .main_file(
            r#"
#[derive(hegel::DefaultGenerator)]
union FloatOrInt {
    f: f32,
    i: i32,
}

fn main() {}
"#,
        )
        .expect_failure("unions")
        .cargo_run(&[]);
}

#[test]
fn test_hegel_test_rejects_self_parameter() {
    TempRustProject::new()
        .test_file(
            "selfparam.rs",
            r#"
struct Foo;

impl Foo {
    #[hegel::test]
    fn my_test(self) {}
}
"#,
        )
        .main_file("fn main() {}")
        .expect_failure("exactly one parameter|self parameter")
        .cargo_test(&[]);
}

#[test]
fn test_hegel_test_rejects_duplicate_test_attribute() {
    TempRustProject::new()
        .test_file(
            "duptest.rs",
            r#"
#[hegel::test]
#[test]
fn my_test(tc: hegel::TestCase) {}
"#,
        )
        .main_file("fn main() {}")
        .expect_failure(r"Remove the \#\[test\] attribute")
        .cargo_test(&[]);
}
