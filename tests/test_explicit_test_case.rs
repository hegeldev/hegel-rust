mod common;

use common::project::TempRustProject;
use common::utils::{assert_matches_regex, expect_panic};
use hegel::TestCase;
use hegel::generators;

// ============================================================
// Compile error tests (via TempRustProject)
// ============================================================

#[test]
fn test_explicit_test_case_on_bare_function() {
    let code = r#"
#[hegel::explicit_test_case(x = 42)]
fn my_func(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("can only be used together with.*hegel::test")
        .cargo_run(&[]);
}

#[test]
fn test_explicit_test_case_wrong_order() {
    let code = r#"
#[hegel::explicit_test_case(x = 42)]
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("must appear below.*hegel::test.*not above")
        .cargo_run(&[]);
}

#[test]
fn test_explicit_test_case_bad_syntax() {
    // Semicolon instead of comma should produce a compile error, not a silent empty case.
    let code = r#"
#[hegel::test]
#[hegel::explicit_test_case(x = 42;)]
fn my_test(tc: hegel::TestCase) {
    let x: i32 = tc.draw(hegel::generators::integers());
    let _ = x;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("expected `,`")
        .cargo_run(&[]);
}

#[test]
fn test_explicit_test_case_empty_args() {
    let code = r#"
#[hegel::test]
#[hegel::explicit_test_case()]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("requires at least one")
        .cargo_run(&[]);
}

#[test]
fn test_explicit_test_case_no_parens() {
    let code = r#"
#[hegel::test]
#[hegel::explicit_test_case]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("requires arguments")
        .cargo_run(&[]);
}

// ============================================================
// Success cases (inline #[hegel::test])
// ============================================================

#[hegel::test]
#[hegel::explicit_test_case(x = true)]
fn test_single_explicit_case(tc: TestCase) {
    let x = tc.draw(generators::booleans());
    let _ = x;
}

#[hegel::test]
#[hegel::explicit_test_case(x = true)]
#[hegel::explicit_test_case(x = false)]
fn test_multiple_explicit_cases(tc: TestCase) {
    let x = tc.draw(generators::booleans());
    let _ = x;
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(x = 42i32)]
fn test_explicit_case_with_property_test(tc: TestCase) {
    let x: i32 = tc.draw(generators::integers());
    assert_eq!(x, x);
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(x = 42u32)]
fn test_explicit_case_type_annotated_draw_uses_name(tc: TestCase) {
    // This verifies the draw is rewritten to draw_named("x", ...) even with
    // a type annotation. If it fell back to "unnamed", the explicit test case
    // would panic with "no value provided for unnamed".
    let x: u32 = tc.draw(generators::integers());
    let _ = x;
}

// ============================================================
// Runtime panic tests
// ============================================================

#[test]
fn test_explicit_draw_unnamed() {
    let etc = hegel::ExplicitTestCase::new().with_value("unnamed", "42", 42i32);
    etc.run(|tc: &hegel::ExplicitTestCase| {
        let x: i32 = tc.draw(generators::integers());
        assert_eq!(x, 42);
    });
}

#[test]
fn test_explicit_draw_silent_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new().with_value("x", "true", true);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: bool = tc.draw_silent(generators::booleans());
            });
        },
        "draw_silent is not supported in explicit test cases",
    );
}

#[test]
fn test_explicit_type_mismatch_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new().with_value("x", "42", 42i32);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                // Try to draw as String instead of i32
                let _: String = tc.draw_named(generators::text(), "x", false);
            });
        },
        "type mismatch",
    );
}

#[test]
fn test_explicit_unused_values_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new()
                .with_value("x", "true", true)
                .with_value("y", "false", false);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: bool = tc.draw_named(generators::booleans(), "x", false);
                // y is never drawn
            });
        },
        "never drawn",
    );
}

#[test]
fn test_explicit_unknown_name_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new().with_value("x", "true", true);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: bool = tc.draw_named(generators::booleans(), "nonexistent", false);
            });
        },
        "no value provided for.*nonexistent",
    );
}

#[test]
fn test_explicit_double_consume_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new().with_value("x", "true", true);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: bool = tc.draw_named(generators::booleans(), "x", false);
                let _: bool = tc.draw_named(generators::booleans(), "x", false);
            });
        },
        "already consumed",
    );
}

// ============================================================
// Output format tests (via TempRustProject)
// ============================================================

#[test]
fn test_explicit_output_format_with_comment() {
    let code = r#"
fn main() {
    let etc = hegel::ExplicitTestCase::new()
        .with_value("x", "compute()", 42i32);
    etc.run(|tc: &hegel::ExplicitTestCase| {
        let _: i32 = tc.draw_named(hegel::generators::integers(), "x", false);
        panic!("intentional");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("intentional")
        .cargo_run(&[]);

    // Source and debug differ, so comment should appear
    assert_matches_regex(&output.stderr, r"let x = compute\(\); // = 42");
}

#[test]
fn test_explicit_output_format_without_comment() {
    let code = r#"
fn main() {
    let etc = hegel::ExplicitTestCase::new()
        .with_value("x", "42", 42i32);
    etc.run(|tc: &hegel::ExplicitTestCase| {
        let _: i32 = tc.draw_named(hegel::generators::integers(), "x", false);
        panic!("intentional");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("intentional")
        .cargo_run(&[]);

    // Source "42" and debug "42" are the same, so no comment
    assert_matches_regex(&output.stderr, r"let x = 42;");
    assert!(
        !output.stderr.contains("// ="),
        "Should not have comment when source matches debug. Actual: {}",
        output.stderr
    );
}

#[test]
fn test_explicit_notes_printed_on_panic() {
    let code = r#"
fn main() {
    let etc = hegel::ExplicitTestCase::new()
        .with_value("x", "42", 42i32);
    etc.run(|tc: &hegel::ExplicitTestCase| {
        let _: i32 = tc.draw_named(hegel::generators::integers(), "x", false);
        tc.note("important debug info");
        panic!("intentional");
    });
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("intentional")
        .cargo_run(&[]);

    assert_matches_regex(&output.stderr, "important debug info");
}

// ============================================================
// Macro integration: output from #[hegel::explicit_test_case]
// ============================================================

#[test]
fn test_macro_explicit_case_output() {
    let code = r#"
#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(x = 42i32)]
fn test_explicit(tc: hegel::TestCase) {
    let x: i32 = tc.draw(hegel::generators::integers());
    panic!("fail: {}", x);
}
"#;
    TempRustProject::new()
        .test_file("test_etc.rs", code)
        .expect_failure("fail: 42")
        .cargo_test(&["--test", "test_etc"]);
}
