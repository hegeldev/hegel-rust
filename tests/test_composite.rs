mod common;

use common::project::TempRustProject;
use hegel::TestCase;
use hegel::generators::integers;

const MISSING_TEST_CASE_PARAMETER: &str = "Functions marked with #[composite] must have a first parameter of type TestCase.";
const MISSING_COMPOSITE_RETURN_TYPE: &str =
    "Functions marked with #[composite] must have an explicit return type.";

#[test]
fn test_successful_expansion() {
    let code = r#"
use hegel::generators::integers;
use hegel::TestCase;

#[hegel::composite]
fn composite_integer_generator(tc: TestCase, n: i32) -> i32 {
    tc.draw(integers::<i32>()) + n
}

fn main() {}
"#;

    let output = TempRustProject::new(code).run();
    assert!(output.status.success());
}

#[test]
fn test_missing_return_type() {
    let code = r#"
use hegel::generators::integers;
use hegel::TestCase;

#[hegel::composite]
fn composite_integer_generator(tc: TestCase, n: i32) {
    tc.draw(integers::<i32>()) + n
}

fn main() {}
"#;

    let output = TempRustProject::new(code).run();
    assert!(!output.status.success());
    assert!(
        output.stderr.contains(MISSING_COMPOSITE_RETURN_TYPE),
        "Expected missing return type error, got: {}",
        output.stderr
    );
}

#[test]
fn test_missing_test_case_parameter() {
    let code = r#"
#[hegel::composite]
fn composite_integer_generator(n: i32) -> i32 {
    n
}

fn main() {}
"#;

    let output = TempRustProject::new(code).run();
    assert!(!output.status.success());
    assert!(
        output.stderr.contains(MISSING_TEST_CASE_PARAMETER),
        "Expected missing return type error, got: {}",
        output.stderr
    );
}

#[hegel::composite]
fn composite_integer_generator(tc: TestCase, lower: i32, upper: i32, offset: i32) -> i32 {
    let x = tc.draw(integers::<i32>().min_value(lower).max_value(upper));
    x + offset
}

#[hegel::test]
fn test_passing_composite_generation(tc: TestCase) {
    let x = tc.draw(composite_integer_generator(0, 100, 1));
    assert!(x > 0);
}
