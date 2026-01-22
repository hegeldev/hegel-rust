mod common;

use common::TempRustProject;
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

    // Expected output format (trimmed):
    //   Generated: 0
    //
    //   thread 'main' (12345) panicked at src/main.rs:7:9:
    //   intentional failure: 0
    //   note: run with `RUST_BACKTRACE=1` ...
    //   Test failed: intentional failure: 0
    //
    //   thread 'main' (12345) panicked at .../embedded.rs:...:
    //   Hegel test failed (exit code 1)
    let expected = Regex::new(concat!(
        r"(?s)",
        r"^Generated: -?\d+\n",
        r"\nthread '.*' \(\d+\) panicked at [^\n]+:\n",
        r"intentional failure: -?\d+\n",
        r"note: run with [^\n]+\n",
        r"Test failed: intentional failure: -?\d+\n",
        r"\nthread '.*' \(\d+\) panicked at [^\n]+:\n",
        r"Hegel test failed \(exit code 1\)",
        r"$",
    )).unwrap();

    assert!(
        expected.is_match(&output.stderr),
        "Output did not match expected format.\n\nActual output:\n{}",
        output.stderr
    );
}
