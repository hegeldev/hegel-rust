mod common;

use common::project::TempRustProject;
use regex::Regex;

const FAILING_TEST_CODE: &str = r#"
use hegel::generators;

fn main() {
    hegel::hegel(|| {
        let x = hegel::draw(&generators::integers::<i32>());
        panic!("intentional failure: {}", x);
    });
}
"#;

#[test]
fn test_failing_test_output() {
    let project = TempRustProject::new(FAILING_TEST_CODE);
    let output = project.cargo_run(&[]);
    assert!(!output.status.success());

    // For example:
    //   thread 'main' (1) panicked at src/main.rs:7:9:
    //   intentional failure: 0
    //   Draw 1: 0
    //   note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
    let expected = Regex::new(concat!(
        r"thread '.*' \(\d+\) panicked at src/main\.rs:\d+:\d+:\n",
        r"intentional failure: -?\d+\n",
        r"Draw 1: -?\d+",
    ))
    .unwrap();

    assert!(
        expected.is_match(&output.stderr),
        "Actual: {}",
        output.stderr
    );
}

#[test]
fn test_failing_test_output_with_backtrace() {
    let output = TempRustProject::new(FAILING_TEST_CODE)
        .env("RUST_BACKTRACE", "1")
        .cargo_run(&[]);
    assert!(!output.status.success());

    // For example:
    //   thread 'main' (1) panicked at src/main.rs:7:9:
    //   intentional failure: 0
    //   Draw 1: 0
    //   stack backtrace:
    //      0: __rustc::rust_begin_unwind
    //      1: core::panicking::panic_fmt
    //      2: temp_hegel_test::main::{{closure}}
    //      ...
    //      N: hegel::runner::handle_connection
    //      ...
    //      M: temp_hegel_test::main
    //      ...
    //   note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
    let expected = Regex::new(concat!(
        r"(?s)",
        r"thread 'main' \(\d+\) panicked at src/main\.rs:\d+:\d+:\n",
        r"intentional failure: -?\d+\n",
        r"Draw 1: -?\d+\n",
        r"stack backtrace:\n",
        r"\s+0: .*\n", // frame 0: panic machinery
        r".*",
        r"\s+1: core::panicking::panic_fmt\n", // frame 1: panic_fmt
        r".*",
        r"\s+2: temp_hegel_test::main::\{\{closure\}\}\n", // frame 2: user's closure
        r".*",
        r"hegel::runner::", // hegel internals appear
        r".*",
        r"temp_hegel_test::main\n", // user's main (not closure)
        r".*",
        r"note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace\.",
    ))
    .unwrap();

    assert!(
        expected.is_match(&output.stderr),
        "Actual: {}",
        output.stderr
    );
}

#[test]
fn test_failing_test_output_with_full_backtrace() {
    let output = TempRustProject::new(FAILING_TEST_CODE)
        .env("RUST_BACKTRACE", "full")
        .cargo_run(&[]);
    assert!(!output.status.success());

    let expected = Regex::new(concat!(
        r"(?s)",
        r"thread 'main' \(\d+\) panicked at src/main\.rs:\d+:\d+:\n",
        r"intentional failure: -?\d+\n",
        r"Draw 1: -?\d+\n",
        r"stack backtrace:\n",
        r"\s+0: .*\n", // starts at frame 0
        r".*",
        r"temp_hegel_test::main::\{\{closure\}\}", // user's closure
        r".*",
        r"hegel::runner::", // hegel internals
        r".*",
        r"temp_hegel_test::main\n", // user's main
        r".*$",
    ))
    .unwrap();

    assert!(
        expected.is_match(&output.stderr),
        "Actual: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Some details are omitted"),
        "Actual: {}",
        output.stderr
    );
}
