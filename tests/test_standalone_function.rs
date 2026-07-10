mod common;

use common::utils::{capture_draw_lines, expect_panic};
use hegel::TestCase;
use hegel::generators as gs;

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

#[hegel::standalone_function(test_cases = 1)]
fn rewriting_fails(tc: TestCase, target: i32) {
    let my_var: i32 = tc.draw(gs::integers());
    panic!("got {} (target {})", my_var, target);
}

#[test]
fn test_standalone_draw_name_rewriting() {
    let lines = capture_draw_lines(|| rewriting_fails(7), "got");
    assert!(
        lines.iter().any(|l| {
            regex::Regex::new(r"let my_var = -?\d+;")
                .unwrap()
                .is_match(l)
        }),
        "expected a rewritten `let my_var = …;` draw line, got {lines:?}"
    );
}

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

#[hegel::standalone_function(test_cases = 1)]
#[hegel::explicit_test_case(x = 99i32)]
fn explicit_value_fails(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    panic!("got {}", x);
}

#[test]
fn test_standalone_explicit_case_uses_provided_value() {
    expect_panic(explicit_value_fails, "got 99");
}

#[test]
fn test_standalone_function_fails_on_failing_property() {
    #[hegel::standalone_function(test_cases = 200)]
    fn always_fails(tc: TestCase) {
        let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
        assert!(x < 0, "unexpectedly got nonneg {}", x);
    }

    expect_panic(always_fails, "unexpectedly got nonneg");
}

// The compile-error cases — no parameters, a return type, combining with
// `#[test]` — live in `tests/ui/standalone_function_*.rs`, where trybuild
// pins their diagnostics.
