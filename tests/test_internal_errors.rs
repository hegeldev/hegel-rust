mod common;

use common::project::TempRustProject;
use common::utils::assert_matches_regex;

// These tests trigger a genuine hegel-internal error by asking the engine
// for an integer and then deserializing it as a bool: the type mismatch
// panics inside hegel's own `test_case.rs` (`deserialize_value`), which is
// the native-engine equivalent of an unexpected backend response.
// (Misconfigured generators like `min_value(100).max_value(10)` are reported
// as clean *usage* errors, not internal errors; see `tests/test_usage_errors.rs`.)
//
// The in-process counterpart lives in `tests/embedded/run_lifecycle_tests.rs`
// (`drive_reraises_hegel_internal_panic_as_internal_error`), which exercises
// the same code path directly for coverage.

#[test]
fn test_propagates_internal_error() {
    let code = r#"
use std::sync::atomic::{AtomicU32, Ordering};
use hegel::ciborium::Value;

static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    let err = std::panic::catch_unwind(|| {
        hegel::hegel(|tc| {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);
            // Draw an integer from the engine but deserialize it as a bool —
            // a type mismatch that panics inside hegel's own source.
            let schema = Value::Map(vec![
                (Value::Text("type".into()), Value::Text("integer".into())),
                (Value::Text("min_value".into()), Value::Integer(42.into())),
                (Value::Text("max_value".into()), Value::Integer(42.into())),
            ]);
            let _: bool = hegel::generate_from_schema(&tc, &schema);
        });
    })
    .unwrap_err();

    let msg = err.downcast_ref::<String>().unwrap();
    assert!(msg.contains("hegel internal error at"));
    assert!(msg.contains("Failed to deserialize value"));
    assert!(CALL_COUNT.load(Ordering::SeqCst) == 1);
}
"#;

    TempRustProject::new().main_file(code).cargo_run(&[]);
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

// Subprocess tests that verify the exact user-visible output format of a
// re-raised internal error.

const INTERNAL_ERROR_CODE: &str = r#"
use hegel::ciborium::Value;

fn main() {
    hegel::hegel(|tc| {
        // Draw an integer from the engine but deserialize it as a bool — a
        // type mismatch that panics inside hegel's own `test_case.rs`.
        let schema = Value::Map(vec![
            (Value::Text("type".into()), Value::Text("integer".into())),
            (Value::Text("min_value".into()), Value::Integer(42.into())),
            (Value::Text("max_value".into()), Value::Integer(42.into())),
        ]);
        let _: bool = hegel::generate_from_schema(&tc, &schema);
    });
}
"#;

#[test]
fn test_internal_error_output() {
    let output = TempRustProject::new()
        .main_file(INTERNAL_ERROR_CODE)
        .env("RUST_BACKTRACE", "0")
        .expect_failure("Failed to deserialize value")
        .cargo_run(&[]);

    assert_matches_regex(
        &output.stderr,
        concat!(
            r"thread '.*'(?: \(\d+\))? panicked at .*src[/\\](?:[A-Za-z_]+[/\\])*(?:runner|run_lifecycle)\.rs:\d+:\d+:\n",
            r"hegel internal error at .*src[/\\](?:[A-Za-z_]+[/\\])*test_case\.rs:\d+:\d+:\n",
            r"Failed to deserialize value:[^\n]*\n",
            r"Value:[^\n]*\n\n",
            r"note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
        ),
    );
}

// With RUST_BACKTRACE=1, the output should include the original backtrace
// (from the actual panic site inside hegel) followed by the re-panic
// backtrace from the default handler.
//
// For example:
//   thread 'main' (N) panicked at .../src/run_lifecycle.rs:NNN:NN:
//   hegel internal error at .../src/test_case.rs:NNN:NN:
//   Failed to deserialize value: invalid type: ...
//   Value: Integer(...)
//
//   original backtrace:
//      0: __rustc::rust_begin_unwind
//      1: core::panicking::panic_fmt
//      ...
//      N: temp_hegel_test_N::main::{{closure}}
//      ...
//
//   stack backtrace:
//      ...
#[test]
fn test_internal_error_output_with_backtrace() {
    let output = TempRustProject::new()
        .main_file(INTERNAL_ERROR_CODE)
        .env("RUST_BACKTRACE", "1")
        .expect_failure("Failed to deserialize value")
        .cargo_run(&[]);

    let closure_name = r"(?:\{closure#0\}|\{\{closure\}\}|closure\$0)";
    assert_matches_regex(
        &output.stderr,
        // Backtrace frame names vary between platforms and across hegel's
        // internal call chain, so we anchor on the stable structure (the two
        // backtrace sections and the user's own `main`/closure frames) rather
        // than on specific hegel-internal symbol names. Symbol qualification
        // also varies: Linux/Windows show fully-qualified names
        // (`temp_hegel_test_N::main::{{closure}}`), while macOS demangles
        // from debuginfo to compact names (`{closure#0}`), so the qualified
        // prefixes are optional.
        &format!(
            concat!(
                r"(?s)",
                // re-panic location from default handler
                r"thread '.*'(?: \(\d+\))? panicked at .*src[/\\](?:[A-Za-z_]+[/\\])*(?:runner|run_lifecycle)\.rs:\d+:\d+:\n",
                // our formatted message: original location + error
                r"hegel internal error at .*src[/\\](?:[A-Za-z_]+[/\\])*test_case\.rs:\d+:\d+:\n",
                r"Failed to deserialize value:[^\n]*\n",
                r"Value:[^\n]*\n",
                r"\n",
                // original backtrace from the actual panic site
                r"original backtrace:\n",
                r"\s+0: .*\n", // frame 0: panic machinery
                r".*",
                r"\s+1: core::panicking::panic_fmt\n", // frame 1: panic_fmt
                r".*",
                r"(?:temp_hegel_test_\d+_\d+::main::)?{closure_name}\n", // user's closure
                r".*",
                r"hegel::(?:[a-z_]+::)*run_test_case", // hegel runner internals
                r".*",
                r"\d+: (?:temp_hegel_test_\d+_\d+::)?main\n", // user's main
                r".*",
                // re-panic backtrace from default handler
                r"\nstack backtrace:\n",
                r".*",
                r"(?:Hegel.*run|drive)[^\n]*\n", // re-panic site (Hegel::run / drive)
                r".*",
                r"note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace\.",
            ),
            closure_name = closure_name,
        ),
    );
}
