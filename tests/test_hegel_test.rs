mod common;

use common::project::TempRustProject;
use hegel::generators;
use hegel::TestCase;

#[hegel::test]
fn test_basic_usage(tc: TestCase) {
    let _ = tc.draw(generators::booleans());
}

#[hegel::test(test_cases = 10)]
fn test_with_settings(tc: TestCase) {
    let _ = tc.draw(generators::booleans());
}

#[test]
fn test_duplicate_test_attribute_compile_error() {
    let code = r#"
use hegel::generators;

#[hegel::test]
#[test]
fn main(tc: hegel::TestCase) {}
"#;
    let output = TempRustProject::new(code).run();
    assert!(!output.status.success());
    assert!(
        output.stderr.contains("Remove the #[test] attribute"),
        "Expected duplicate test error, got: {}",
        output.stderr
    );
}

#[test]
fn test_params_compile_error() {
    // Zero parameters should be rejected
    let code_zero = r#"
use hegel::generators;

#[hegel::test]
fn main() {
}
"#;
    let output = TempRustProject::new(code_zero).run();
    assert!(!output.status.success());
    assert!(
        output
            .stderr
            .contains("must take exactly one parameter of type hegel::TestCase"),
        "Expected parameter error for zero params, got: {}",
        output.stderr
    );

    // Two parameters should be rejected
    let code_two = r#"
use hegel::generators;

#[hegel::test]
fn main(tc: hegel::TestCase, x: bool) {
    let _ = (tc, x);
}
"#;
    let output = TempRustProject::new(code_two).run();
    assert!(!output.status.success());
    assert!(
        output
            .stderr
            .contains("must take exactly one parameter of type hegel::TestCase"),
        "Expected parameter error for two params, got: {}",
        output.stderr
    );
}
