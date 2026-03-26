mod common;

use common::project::TempRustProject;
use common::utils::assert_matches_regex;
use hegel::generators::{self, Generator};
use hegel::{Hegel, Settings};

const FAILING_TEST_CODE: &str = r#"
use hegel::generators;

fn main() {
    hegel::hegel(|tc| {
        let x = tc.draw(generators::integers::<i32>());
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
    //   Draw 1: 0
    //   thread 'main' (1) panicked at src/main.rs:7:9:
    //   intentional failure: 0
    //   note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
    assert_matches_regex(
        &output.stderr,
        concat!(
            r"Draw 1: -?\d+\n",
            r"thread '.*' \(\d+\) panicked at src/main\.rs:\d+:\d+:\n",
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

    // Rust >= 1.92 uses {closure#0}, older stable uses {{closure}}
    let closure_name = r"(\{closure#0\}|\{\{closure\}\})";
    // For example:
    //   Draw 1: 0
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
                r"Draw 1: -?\d+\n",
                r"thread 'main' \(\d+\) panicked at src/main\.rs:\d+:\d+:\n",
                r"intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r"\s+0: .*\n", // frame 0: panic machinery
                r".*",
                r"\s+1: core::panicking::panic_fmt\n", // frame 1: panic_fmt
                r".*",
                r"\s+2: temp_hegel_test_\d+::main::{closure_name}\n", // frame 2: user's closure
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

    // Rust >= 1.92 uses {closure#0}, older stable uses {{closure}}
    let closure_name = r"(\{closure#0\}|\{\{closure\}\})";
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                r"Draw 1: -?\d+\n",
                r"thread 'main' \(\d+\) panicked at src/main\.rs:\d+:\d+:\n",
                r"intentional failure: -?\d+\n",
                r"stack backtrace:\n",
                r"\s+0: .*\n", // starts at frame 0
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

/// Exercise the in-process failure path to cover panic info capture,
/// test result handling, and note() output during final replay.
#[test]
#[should_panic(expected = "Property test failed")]
fn test_in_process_failure_exercises_panic_path() {
    Hegel::new(|tc| {
        let x: i32 = tc.draw(generators::integers());
        tc.note(&format!("testing note output: x={}", x));
        panic!("intentional-test-failure-42: {}", x);
    })
    .settings(Settings::new().test_cases(10).derandomize(true))
    .run();
}

/// Exercise the backtrace output path by running a failing test with
/// RUST_BACKTRACE=1 to force Backtrace::capture() to actually capture.
#[test]
fn test_in_process_failure_with_backtrace() {
    // SAFETY: serialized by ENV_TEST_MUTEX
    let _guard = hegel::ENV_TEST_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let original = std::env::var("RUST_BACKTRACE").ok();
    unsafe { std::env::set_var("RUST_BACKTRACE", "1") };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let _: bool = tc.draw(generators::booleans());
            panic!("backtrace-coverage-test-99");
        })
        .settings(Settings::new().test_cases(5).derandomize(true))
        .run();
    }));

    match original {
        Some(v) => unsafe { std::env::set_var("RUST_BACKTRACE", v) },
        None => unsafe { std::env::remove_var("RUST_BACKTRACE") },
    }

    assert!(result.is_err(), "expected Hegel::run to panic");
}

/// Exercise the verbosity debug path
#[test]
#[should_panic(expected = "Property test failed")]
fn test_in_process_failure_with_debug_verbosity() {
    Hegel::new(|tc| {
        let _: bool = tc.draw(generators::booleans());
        panic!("debug-verbosity-test-failure");
    })
    .settings(
        Settings::new()
            .test_cases(5)
            .derandomize(true)
            .verbosity(hegel::Verbosity::Debug),
    )
    .run();
}

/// Force the StopTest/overflow path by drawing heavily from span-based
/// generators (flat_map) in a property that fails. During shrinking, the
/// server will truncate data, causing StopTest errors in start_span and
/// collection operations.
#[test]
#[should_panic(expected = "Property test failed")]
fn test_overflow_exercises_stop_test_paths() {
    Hegel::new(|tc| {
        // Use flat_map to force span-based generation (not schema-based).
        // This means start_span/stop_span are called, and during shrinking
        // the server may exhaust data mid-span, triggering StopTest.
        for _ in 0..10 {
            let inner: Vec<String> = tc.draw(
                generators::vecs(
                    generators::integers::<usize>()
                        .min_value(1)
                        .max_value(5)
                        .flat_map(|n| generators::text().min_size(n).max_size(n)),
                )
                .min_size(1)
                .max_size(5),
            );
            // Fail unconditionally — forces shrinking which triggers StopTest
            if !inner.is_empty() {
                panic!("overflow-stop-test-coverage-77");
            }
        }
    })
    .settings(Settings::new().test_cases(20).derandomize(true))
    .run();
}
