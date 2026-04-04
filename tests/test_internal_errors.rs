mod common;

use common::project::TempRustProject;
use common::utils::assert_matches_regex;

#[test]
fn test_propagates_server_error() {
    let code = r#"
use std::sync::atomic::{AtomicU32, Ordering};
use hegel::generators as gs;

static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    let err = std::panic::catch_unwind(|| {
        hegel::hegel(|tc| {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);
            let _ = tc.draw(gs::booleans());
        });
    })
    .unwrap_err();

    let msg = err.downcast_ref::<String>().unwrap();
    assert!(msg.contains("RequestError"));
    assert!(CALL_COUNT.load(Ordering::SeqCst) == 1);
}
"#;

    TempRustProject::new()
        .main_file(code)
        .env("HEGEL_PROTOCOL_TEST_MODE", "error_response")
        .cargo_run(&[]);
}

#[test]
fn test_generator_error_raises_immediately() {
    let code = r#"
use std::sync::atomic::{AtomicU32, Ordering};
use hegel::generators as gs;

static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    let err = std::panic::catch_unwind(|| {
        hegel::hegel(|tc| {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);
            let _ = tc.draw(gs::integers::<i32>().min_value(100).max_value(10));
        });
    })
    .unwrap_err();

    let msg = err.downcast_ref::<String>().unwrap();
    assert!(msg.contains("Cannot have max_value < min_value"));
    assert!(CALL_COUNT.load(Ordering::SeqCst) == 1);
}
"#;

    TempRustProject::new().main_file(code).cargo_run(&[]);
}

const INTERNAL_ERROR_CODE: &str = r#"
use hegel::generators as gs;

fn main() {
    hegel::hegel(|tc| {
        let _ = tc.draw(gs::integers::<i32>().min_value(100).max_value(10));
    });
}
"#;

#[test]
fn test_internal_error_output() {
    let output = TempRustProject::new()
        .main_file(INTERNAL_ERROR_CODE)
        .env("RUST_BACKTRACE", "0")
        .expect_failure("Cannot have max_value < min_value")
        .cargo_run(&[]);

    assert_matches_regex(
        &output.stderr,
        concat!(
            r"thread '.*' \(\d+\) panicked at .*src/runner\.rs:\d+:\d+:\n",
            r"hegel internal error at .*src/generators/numeric\.rs:\d+:\d+:\n",
            r"Cannot have max_value < min_value\n\n",
            r"note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
        ),
    );
}

// With RUST_BACKTRACE=1, the output should include the original backtrace
// followed by the re-panic backtrace from the default handler.
//
// For example:
//   thread 'main' (N) panicked at .../src/runner.rs:NNN:NN:
//   hegel internal error at .../src/generators/numeric.rs:72:9:
//   Cannot have max_value < min_value
//
//   original backtrace:
//      0: __rustc::rust_begin_unwind
//      1: core::panicking::panic_fmt
//      2: hegel::generators::numeric::IntegerGenerator<T>::build_schema
//      3: <...IntegerGenerator<T> as ...Generator<T>>::do_draw
//      4: hegel::test_case::TestCase::draw
//      5: temp_hegel_test_N::main::{{closure}}
//      ...
//
//   stack backtrace:
//      ...
#[test]
fn test_internal_error_output_with_backtrace() {
    let output = TempRustProject::new()
        .main_file(INTERNAL_ERROR_CODE)
        .env("RUST_BACKTRACE", "1")
        .expect_failure("Cannot have max_value < min_value")
        .cargo_run(&[]);

    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\})";
    assert_matches_regex(
        &output.stderr,
        &format!(
            concat!(
                r"(?s)",
                // re-panic location from default handler
                r"thread '.*' \(\d+\) panicked at .*src/runner\.rs:\d+:\d+:\n",
                // our formatted message: original location + error
                r"hegel internal error at .*src/generators/numeric\.rs:\d+:\d+:\n",
                r"Cannot have max_value < min_value\n",
                r"\n",
                // original backtrace from the actual panic site
                r"original backtrace:\n",
                r"\s+0: .*\n",
                r".*",
                r"\s+1: core::panicking::panic_fmt\n",
                r".*",
                r"\s+2: hegel::generators::numeric::IntegerGenerator<T>::build_schema\n",
                r".*",
                r"Generator<T>>::do_draw\n",
                r".*",
                r"hegel::test_case::TestCase::draw\n",
                r".*",
                r"temp_hegel_test_\d+::main::{closure_name}\n",
                r".*",
                r"hegel::runner::run_test_case",
                r".*",
                r"temp_hegel_test_\d+::main\n",
                r".*",
                // re-panic backtrace from default handler
                r"\nstack backtrace:\n",
                r".*",
                r"hegel::runner::Hegel<F>::run\n",
                r".*",
                r"note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace\.",
            ),
            closure_name = closure_name,
        ),
    );
}
