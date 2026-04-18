mod common;

use std::sync::OnceLock;

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
    // This verifies the draw is rewritten to __draw_named("x", ...) even with
    // a type annotation. If it fell back to "unnamed", the explicit test case
    // would panic with "no value provided for unnamed".
    let x: u32 = tc.draw(generators::integers());
    let _ = x;
}

#[derive(Debug, Clone, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(p = Point { x: 3, y: 4 })]
fn test_explicit_case_with_user_defined_struct(tc: TestCase) {
    let p: Point = tc.draw(generators::just(Point { x: 0, y: 0 }));
    assert_eq!(p, p);
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(p = Point { x: 3, y: 4 }, q = Point { x: -1, y: 0 })]
fn test_explicit_case_with_multiple_structs(tc: TestCase) {
    let p: Point = tc.draw(generators::just(Point { x: 0, y: 0 }));
    let q: Point = tc.draw(generators::just(Point { x: 0, y: 0 }));
    let _ = (p, q);
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(n = vec![1i32, 2, 3].into_iter().sum::<i32>())]
fn test_explicit_case_with_function_evaluation(tc: TestCase) {
    let n: i32 = tc.draw(generators::integers());
    let _ = n;
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(s = ["hello", "world"].join(" "))]
fn test_explicit_case_with_method_chain(tc: TestCase) {
    let s: String = tc.draw(generators::text());
    let _ = s;
}

// ============================================================
// Runtime panic tests
// ============================================================

#[test]
fn test_explicit_draw_unnamed() {
    let etc = hegel::ExplicitTestCase::new().with_value("draw", "42", 42i32);
    etc.run(|tc: &hegel::ExplicitTestCase| {
        let x: i32 = tc.draw(generators::integers());
        assert_eq!(x, 42);
    });
}

#[test]
fn test_explicit_note() {
    let etc = hegel::ExplicitTestCase::new().with_value("x", "true", true);
    etc.run(|tc: &hegel::ExplicitTestCase| {
        let _: bool = tc.__draw_named(generators::booleans(), "x", false);
        tc.note("some note");
    });
}

#[test]
fn test_explicit_notes_printed_on_panic_inline() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new().with_value("x", "42", 42i32);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: i32 = tc.__draw_named(generators::integers(), "x", false);
                tc.note("a note");
                panic!("intentional");
            });
        },
        "intentional",
    );
}

#[test]
fn test_explicit_assume_passes() {
    let etc = hegel::ExplicitTestCase::new();
    etc.assume(true);
}

#[test]
fn test_explicit_assume_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new();
            etc.assume(false);
        },
        "__HEGEL_ASSUME_FAIL",
    );
}

#[test]
fn test_explicit_start_span_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new();
            etc.start_span(0);
        },
        "start_span is not supported in explicit test cases",
    );
}

#[test]
fn test_explicit_stop_span_panics() {
    expect_panic(
        || {
            let etc = hegel::ExplicitTestCase::new();
            etc.stop_span(false);
        },
        "stop_span is not supported in explicit test cases",
    );
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
                let _: String = tc.__draw_named(generators::text(), "x", false);
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
                let _: bool = tc.__draw_named(generators::booleans(), "x", false);
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
                let _: bool = tc.__draw_named(generators::booleans(), "nonexistent", false);
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
                let _: bool = tc.__draw_named(generators::booleans(), "x", false);
                let _: bool = tc.__draw_named(generators::booleans(), "x", false);
            });
        },
        "already consumed",
    );
}

// ============================================================
// Output format tests (via TempRustProject)
//
// All three share a single main.rs that dispatches on the
// HEGEL_TEST_SCENARIO env var, so the wrapper crate is only built
// once across the three #[test]s.
// ============================================================

fn output_format_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| {
        let code = r#"
fn main() {
    match std::env::var("HEGEL_TEST_SCENARIO").as_deref() {
        Ok("with_comment") => {
            let etc = hegel::ExplicitTestCase::new()
                .with_value("x", "compute()", 42i32);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: i32 = tc.__draw_named(hegel::generators::integers(), "x", false);
                panic!("intentional");
            });
        }
        Ok("without_comment") => {
            let etc = hegel::ExplicitTestCase::new()
                .with_value("x", "42", 42i32);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: i32 = tc.__draw_named(hegel::generators::integers(), "x", false);
                panic!("intentional");
            });
        }
        Ok("notes") => {
            let etc = hegel::ExplicitTestCase::new()
                .with_value("x", "42", 42i32);
            etc.run(|tc: &hegel::ExplicitTestCase| {
                let _: i32 = tc.__draw_named(hegel::generators::integers(), "x", false);
                tc.note("important debug info");
                panic!("intentional");
            });
        }
        other => panic!("unknown HEGEL_TEST_SCENARIO: {:?}", other),
    }
}
"#;
        TempRustProject::new().main_file(code)
    })
}

#[test]
fn test_explicit_output_format_with_comment() {
    let output = output_format_project()
        .invoke()
        .env("HEGEL_TEST_SCENARIO", "with_comment")
        .expect_failure("intentional")
        .cargo_run(&[]);

    // Source and debug differ, so comment should appear
    assert_matches_regex(&output.stderr, r"let x = compute\(\); // = 42");
}

#[test]
fn test_explicit_output_format_without_comment() {
    let output = output_format_project()
        .invoke()
        .env("HEGEL_TEST_SCENARIO", "without_comment")
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
    let output = output_format_project()
        .invoke()
        .env("HEGEL_TEST_SCENARIO", "notes")
        .expect_failure("intentional")
        .cargo_run(&[]);

    assert_matches_regex(&output.stderr, "important debug info");
}

// ============================================================
// Macro integration: output from #[hegel::explicit_test_case]
//
// All three macro-expansion scenarios live in a single test file
// built once; each #[test] invokes `cargo test` with a per-scenario
// name filter so the panic output it asserts on is isolated.
// ============================================================

fn macro_integration_project() -> &'static TempRustProject {
    static PROJECT: OnceLock<TempRustProject> = OnceLock::new();
    PROJECT.get_or_init(|| {
        let code = r#"
use hegel::generators as gs;

#[derive(Debug, Clone, PartialEq)]
struct Point { x: i32, y: i32 }

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(x = 42i32)]
fn macro_output(tc: hegel::TestCase) {
    let x: i32 = tc.draw(gs::integers());
    panic!("fail: {}", x);
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(p = Point { x: 3, y: 4 })]
fn macro_struct(tc: hegel::TestCase) {
    let p: Point = tc.draw(gs::just(Point { x: 0, y: 0 }));
    panic!("fail: {:?}", p);
}

#[hegel::test(test_cases = 1)]
#[hegel::explicit_test_case(n = vec![10i32, 20, 30].into_iter().sum::<i32>())]
fn macro_computed(tc: hegel::TestCase) {
    let n: i32 = tc.draw(gs::integers());
    panic!("fail: {}", n);
}
"#;
        TempRustProject::new().test_file("test_macro.rs", code)
    })
}

#[test]
fn test_macro_explicit_case_output() {
    macro_integration_project()
        .invoke()
        .expect_failure("fail: 42")
        .cargo_test(&["--test", "test_macro", "macro_output"]);
}

#[test]
fn test_macro_explicit_case_with_struct() {
    macro_integration_project()
        .invoke()
        .expect_failure(r"fail: Point \{ x: 3, y: 4 \}")
        .cargo_test(&["--test", "test_macro", "macro_struct"]);
}

#[test]
fn test_macro_explicit_case_with_computed_expression() {
    macro_integration_project()
        .invoke()
        .expect_failure("fail: 60")
        .cargo_test(&["--test", "test_macro", "macro_computed"]);
}
