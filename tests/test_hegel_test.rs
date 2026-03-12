mod common;

use common::project::TempRustProject;
use hegel::generators;

#[hegel::test]
fn test_basic_usage() {
    let _ = hegel::draw(&generators::booleans());
}

#[hegel::test(test_cases = 10)]
fn test_with_settings() {
    let _ = hegel::draw(&generators::booleans());
}

#[test]
#[should_panic(expected = "draw() cannot be called outside of a Hegel test")]
fn test_draw_outside_test_panics() {
    hegel::draw(&generators::booleans());
}

#[test]
#[should_panic(expected = "assume() cannot be called outside of a Hegel test")]
fn test_assume_outside_test_panics() {
    hegel::assume(true);
}

#[test]
#[should_panic(expected = "note() cannot be called outside of a Hegel test")]
fn test_note_outside_test_panics() {
    hegel::note("a note");
}

#[test]
fn test_duplicate_test_attribute_compile_error() {
    let code = r#"
use hegel::generators;

#[hegel::test]
#[test]
fn main() {}
"#;
    let output = TempRustProject::new(code).cargo_run(&[]);
    assert!(!output.status.success());
    assert!(
        output.stderr.contains("Remove the #[test] attribute"),
        "Expected duplicate test error, got: {}",
        output.stderr
    );
}

#[test]
fn test_params_compile_error() {
    let code = r#"
use hegel::generators;

#[hegel::test]
fn main(x: bool) {
    let _ = x;
}
"#;
    let output = TempRustProject::new(code).cargo_run(&[]);
    assert!(!output.status.success());
    assert!(
        output.stderr.contains("must not have parameters"),
        "Expected parameter error, got: {}",
        output.stderr
    );
}
