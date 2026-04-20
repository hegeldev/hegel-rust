mod common;

use common::project::TempRustProject;
use common::utils::assert_matches_regex;

const BASIC_MAIN: &str = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 7)]
fn main(tc: TestCase) {
    let _: i32 = tc.draw(gs::integers());
    eprintln!("ran");
}
"#;

#[test]
fn test_basic_main_runs() {
    let output = TempRustProject::new().main_file(BASIC_MAIN).cargo_run(&[]);
    // The #[hegel::main] wraps the body in an FnMut closure invoked test_cases times.
    // So `ran` should appear 7 times.
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 7, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_cli_overrides_test_cases() {
    let output =
        TempRustProject::new()
            .main_file(BASIC_MAIN)
            .cargo_run(&["--", "--test-cases", "3"]);
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 3, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_default_matches_attribute() {
    // No CLI args => defaults are whatever the attribute said (7).
    let output = TempRustProject::new().main_file(BASIC_MAIN).cargo_run(&[]);
    let count = output.stderr.matches("ran").count();
    assert_eq!(count, 7, "stderr:\n{}", output.stderr);
}

#[test]
fn test_main_unknown_arg_exits_with_error() {
    let output = TempRustProject::new()
        .main_file(BASIC_MAIN)
        .expect_failure("Unknown argument")
        .cargo_run(&["--", "--not-a-real-arg"]);
    let _ = output;
}

#[test]
fn test_main_help_exits_cleanly() {
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main]
fn main(tc: TestCase) {
    let _: bool = tc.draw(gs::booleans());
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .cargo_run(&["--", "--help"]);
    assert!(
        output.stdout.contains("Usage:"),
        "stdout did not contain Usage: {}",
        output.stdout
    );
}

#[test]
fn test_main_failing_property_exits_nonzero() {
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 100)]
fn main(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(50));
    assert!(x < 0, "got nonneg {}", x);
}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("Property test failed")
        .cargo_run(&[]);
}

#[test]
fn test_main_draw_name_rewriting() {
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 1)]
fn main(tc: TestCase) {
    let my_var: i32 = tc.draw(gs::integers());
    panic!("boom {}", my_var);
}
"#;
    let output = TempRustProject::new()
        .main_file(code)
        .expect_failure("boom")
        .cargo_run(&[]);
    assert_matches_regex(&output.stderr, r"let my_var = -?\d+;");
}

#[test]
fn test_main_explicit_test_case() {
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 1)]
#[hegel::explicit_test_case(x = 77i32)]
fn main(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    if x == 77 {
        panic!("got explicit value");
    }
}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("got explicit value")
        .cargo_run(&[]);
}

#[test]
fn test_main_verbosity_override() {
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 1)]
fn main(tc: TestCase) {
    let _: bool = tc.draw(gs::booleans());
}
"#;
    // Debug verbosity prints REQUEST/RESPONSE lines (server) or test case status (native).
    let output = TempRustProject::new().main_file(code).cargo_run(&[
        "--",
        "--verbosity",
        "debug",
        "--test-cases",
        "1",
    ]);
    assert!(
        output.stderr.contains("REQUEST:")
            || output.stderr.contains("run_test response")
            || output.stderr.contains("test case #"),
        "Expected debug output, got: {}",
        output.stderr
    );
}

#[test]
fn test_main_seed_override() {
    let code = r#"
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 1)]
fn main(tc: TestCase) {
    let _: bool = tc.draw(gs::booleans());
}
"#;
    // With a fixed seed, the run should complete successfully.
    let output = TempRustProject::new()
        .main_file(code)
        .cargo_run(&["--", "--seed", "42"]);
    let _ = output;
}

#[test]
fn test_main_no_params_compile_error() {
    let code = r#"
#[hegel::main]
fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("must take exactly one parameter")
        .cargo_run(&[]);
}
