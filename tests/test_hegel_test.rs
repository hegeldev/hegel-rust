mod common;

use common::project::TempRustProject;
use hegel::generators;

#[hegel::test]
#[test]
fn test_basic_usage() {
    let _: i32 = hegel::draw(&generators::integers());
}

#[hegel::test(test_cases = 10)]
#[test]
fn test_with_settings() {
    let _: bool = hegel::draw(&generators::booleans());
}

#[test]
fn test_params_compile_error() {
    let code = r#"
use hegel::generators;

#[hegel::test]
fn main(x: i32) {
    let _ = x;
}
"#;
    let output = TempRustProject::new(code).run();
    assert!(!output.status.success());
    assert!(
        output.stderr.contains("must not have parameters"),
        "Expected parameter error, got: {}",
        output.stderr
    );
}
