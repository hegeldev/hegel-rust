mod common;

use common::project::TempRustProject;
use common::utils::{assert_matches_regex, expect_panic};
use hegel::TestCase;
use hegel::generators as gs;

// ============================================================
// Success cases using the macro inline
// ============================================================

#[hegel::standalone_function(test_cases = 5)]
fn standalone_no_args(tc: TestCase) {
    let _: bool = tc.draw(gs::booleans());
}

#[test]
fn test_standalone_no_args_runs() {
    standalone_no_args();
}

#[hegel::standalone_function(test_cases = 5)]
fn standalone_with_copy_arg(tc: TestCase, target: i32) {
    let x: i32 = tc.draw(gs::integers::<i32>());
    assert!(x.checked_add(target).is_some() || x.checked_add(target).is_none());
}

#[test]
fn test_standalone_with_copy_arg_runs() {
    standalone_with_copy_arg(42);
}

#[hegel::standalone_function(test_cases = 5)]
fn standalone_with_string_arg(tc: TestCase, prefix: String) {
    let x: i32 = tc.draw(gs::integers::<i32>());
    assert!(format!("{}-{}", prefix, x).contains(&prefix));
}

#[test]
fn test_standalone_with_string_arg_runs() {
    standalone_with_string_arg("hello".to_string());
}

// ============================================================
// Draw name rewriting
// ============================================================

#[test]
fn test_standalone_draw_name_rewriting() {
    // Use TempRustProject so we can observe the actual rewritten output on failure.
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::standalone_function(test_cases = 1)]
fn fails(tc: TestCase, target: i32) {
    let my_var: i32 = tc.draw(gs::integers());
    panic!("got {} (target {})", my_var, target);
}

fn main() {
    fails(7);
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("got")
        .cargo_run(&[]);

    // Draw rewriting should make the printed variable use the user's name.
    assert_matches_regex(&output.stderr, r"let my_var = -?\d+;");
}

// ============================================================
// Explicit test cases
// ============================================================

#[hegel::standalone_function(test_cases = 1)]
#[hegel::explicit_test_case(x = 42i32)]
fn standalone_with_explicit_case(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    let _ = x;
}

#[test]
fn test_standalone_explicit_case_runs() {
    standalone_with_explicit_case();
}

#[test]
fn test_standalone_explicit_case_uses_provided_value() {
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::standalone_function(test_cases = 1)]
#[hegel::explicit_test_case(x = 99i32)]
fn fails(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    panic!("got {}", x);
}

fn main() {
    fails();
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("got 99")
        .cargo_run(&[]);
    let _ = output;
}

// ============================================================
// Panic on failing property
// ============================================================

#[test]
fn test_standalone_function_fails_on_failing_property() {
    #[hegel::standalone_function(test_cases = 200)]
    fn always_fails(tc: TestCase) {
        let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
        assert!(x < 0, "unexpectedly got nonneg {}", x);
    }

    expect_panic(always_fails, "Property test failed");
}

// ============================================================
// Compile errors
// ============================================================

#[test]
fn test_standalone_function_no_params_compile_error() {
    let code = r#"
#[hegel::standalone_function]
fn bad() {}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("at least one parameter")
        .cargo_run(&[]);
}

#[test]
fn test_standalone_function_return_type_compile_error() {
    let code = r#"
#[hegel::standalone_function]
fn bad(tc: hegel::TestCase) -> i32 { 0 }

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("must not have a return type")
        .cargo_run(&[]);
}

#[test]
fn test_standalone_function_with_test_attribute_compile_error() {
    let code = r#"
#[hegel::standalone_function]
#[test]
fn bad(tc: hegel::TestCase) {}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("cannot be combined with.*test")
        .cargo_run(&[]);
}
