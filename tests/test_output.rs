mod common;

use common::project::TempRustProject;
use common::utils::assert_matches_regex;

const FAILING_TEST_CODE: &str = r#"
use hegel::generators as gs;

fn main() {
    hegel::hegel(|tc| {
        let x = tc.draw(gs::integers::<i32>());
        panic!("intentional failure: {}", x);
    });
}
"#;

#[test]
fn test_failing_test_output() {
    let output = TempRustProject::new()
        .main_file(FAILING_TEST_CODE)
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    // For example:
    //   let draw_1 = 0;
    //   thread 'main' (1) panicked at src/main.rs:7:9:
    //   intentional failure: 0
    //   note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
    assert_matches_regex(
        &output.stderr,
        concat!(
            r"let draw_1 = -?\d+;\n",
            r"thread '.*' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
            r"intentional failure: -?\d+",
        ),
    );
}

#[test]
fn test_failing_test_output_with_backtrace() {
    let output = TempRustProject::new()
        .main_file(FAILING_TEST_CODE)
        .env("RUST_BACKTRACE", "1")
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    // We've seen `{{closure}}` on stable Linux and `{closure#0}` on nightly and
    // macOS stable (the exact conditions aren't fully understood). Accept both.
    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\}|closure\$0)";
    // For example:
    //   let draw_1 = 0;
    //   thread 'main' (1) panicked at src/main.rs:7:9:
    //   intentional failure: 0
    //   stack backtrace:
    //      0: __rustc::rust_begin_unwind
    //      1: core::panicking::panic_fmt
    //      2: temp_hegel_test_N::main::{{closure}}
    //      ...
    //      N: hegel::runner::handle_connection
    //      ...
    //      M: temp_hegel_test_N::main
    //      ...
    //   note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                r"let draw_1 = -?\d+;\n",
                r"thread 'main' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
                r"intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r".*",
                r"core::panicking::panic_fmt\n", // panic_fmt frame
                r".*",
                r"temp_hegel_test_\d+::main::{closure_name}\n", // user's closure
                r".*",
                r"hegel::runner::", // hegel internals appear
                r".*",
                r"temp_hegel_test_\d+::main\n", // user's main (not closure)
                r".*",
                r"note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace\.",
            ),
            closure_name = closure_name,
        ),
    );
}

#[test]
fn test_failing_test_output_with_full_backtrace() {
    let output = TempRustProject::new()
        .main_file(FAILING_TEST_CODE)
        .env("RUST_BACKTRACE", "full")
        .expect_failure("intentional failure")
        .cargo_run(&[]);

    // We've seen `{{closure}}` on stable Linux and `{closure#0}` on nightly and
    // macOS stable (the exact conditions aren't fully understood). Accept both.
    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\}|closure\$0)";
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                r"let draw_1 = -?\d+;\n",
                r"thread 'main' \(\d+\) panicked at src[/\\]main\.rs:\d+:\d+:\n",
                r"intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r".*",
                r"temp_hegel_test_\d+::main::{closure_name}", // user's closure
                r".*",
                r"hegel::runner::", // hegel internals
                r".*",
                r"temp_hegel_test_\d+::main\n", // user's main
                r".*$",
            ),
            closure_name = closure_name,
        ),
    );
    assert!(
        !output.stderr.contains("Some details are omitted"),
        "Actual: {}",
        output.stderr
    );
}
