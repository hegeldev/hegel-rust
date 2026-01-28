mod common;

use common::project::TempRustProject;
use regex::Regex;

#[test]
fn test_failing_test_output() {
    let project = TempRustProject::new(
        r#"
use hegel::gen::{self, Generate};

fn main() {
    hegel::hegel(|| {
        let x = gen::integers::<i32>().generate();
        panic!("intentional failure: {}", x);
    });
}
"#,
    );

    let output = project.run();
    assert!(!output.status.success());

    // For example:
    //   thread 'main' panicked at src/main.rs:7:9:
    //   intentional failure: 0
    //   Generated: 0
    let expected = Regex::new(concat!(
        r"^thread '.*' panicked at src/main\.rs:\d+:\d+:\n",
        r"intentional failure: -?\d+\n",
        r"Generated: -?\d+$",
    ))
    .unwrap();

    assert!(
        expected.is_match(&output.stderr),
        "Output did not match expected format.\n\nActual output:\n{}",
        output.stderr
    );
}
