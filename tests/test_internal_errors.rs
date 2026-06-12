//! Failures of Hegel itself — a broken internal assertion, a framework bug
//! detected mid-draw, or an unexpected panic from inside hegel's own source
//! — are failures of Hegel, not of the property under test. They must abort
//! the run immediately — not be classified as a counterexample, shrunk for
//! the full shrink budget, and reported with a reproducer blob.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};

use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase, Verbosity};

use common::project::TempRustProject;
use common::utils::assert_matches_regex;

// ── explicit internal assertions (`hegel_internal_assert!`) ───────────────

#[test]
fn internal_errors_abort_the_run_without_shrinking() {
    let runs = AtomicUsize::new(0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(|tc: TestCase| {
            runs.fetch_add(1, Ordering::SeqCst);
            // Misuse `__draw_named`: the same name with inconsistent
            // `repeatable` flags is an internal-invariant violation (the
            // rewrite macro can never produce it).
            tc.__draw_named(gs::booleans(), "x", true);
            tc.__draw_named(gs::booleans(), "x", false);
        })
        .settings(
            Settings::new()
                .database(None)
                .derandomize(true)
                .test_cases(50)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    }));

    let payload = result.expect_err("an internal error must fail the run");
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        msg.contains("bug in hegel"),
        "expected the bug-report framing, got: {msg:?}"
    );
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "an internal error must abort the run on the spot, not be shrunk"
    );
}

#[test]
fn repeated_non_repeatable_draw_name_is_an_internal_error() {
    // The other `record_named_draw` invariant: a non-repeatable name used
    // twice. Also must abort rather than shrink.
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(|tc: TestCase| {
            tc.__draw_named(gs::booleans(), "y", false);
            tc.__draw_named(gs::booleans(), "y", false);
        })
        .settings(
            Settings::new()
                .database(None)
                .derandomize(true)
                .test_cases(50)
                .verbosity(Verbosity::Quiet),
        )
        .run();
    }));
    let payload = result.expect_err("an internal error must fail the run");
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        msg.contains("used more than once but repeatable is false"),
        "{msg:?}"
    );
    assert!(msg.contains("bug in hegel"), "{msg:?}");
}

// ── unexpected panics from inside hegel's own source ──────────────────────
//
// These tests trigger a genuine hegel-internal panic by drawing from a
// deferred generator that was never `set()`: a raw `panic!` from inside
// hegel's own `generators/deferred.rs` — as opposed to an explicit
// `hegel_internal_error!` raise (covered above), which unwinds as a typed
// payload and never reaches the location-detection path.
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
use hegel::generators as gs;

static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    let err = std::panic::catch_unwind(|| {
        hegel::hegel(|tc| {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);
            // Drawing from a deferred generator that was never `set()`
            // raw-panics from inside hegel's own source.
            tc.draw(gs::deferred::<bool>().generator());
        });
    })
    .unwrap_err();

    let msg = err.downcast_ref::<String>().unwrap();
    assert!(msg.contains("hegel internal error at"));
    assert!(msg.contains("DeferredGenerator has not been set"));
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
use hegel::generators as gs;

fn main() {
    hegel::hegel(|tc| {
        // Drawing from a deferred generator that was never `set()`
        // raw-panics from inside hegel's own `generators/deferred.rs`.
        tc.draw(gs::deferred::<bool>().generator());
    });
}
"#;

#[test]
fn test_internal_error_output() {
    let output = TempRustProject::new()
        .main_file(INTERNAL_ERROR_CODE)
        .env("RUST_BACKTRACE", "0")
        .expect_failure("DeferredGenerator has not been set")
        .cargo_run(&[]);

    assert_matches_regex(
        &output.stderr,
        concat!(
            r"thread '.*'(?: \(\d+\))? panicked at .*src[/\\](?:[A-Za-z_]+[/\\])*(?:runner|run_lifecycle)\.rs:\d+:\d+:\n",
            r"hegel internal error at .*src[/\\](?:[A-Za-z_]+[/\\])*deferred\.rs:\d+:\d+:\n",
            r"DeferredGenerator has not been set\n\n",
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
//   hegel internal error at .../src/generators/deferred.rs:NN:NN:
//   DeferredGenerator has not been set
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
        .expect_failure("DeferredGenerator has not been set")
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
                r"hegel internal error at .*src[/\\](?:[A-Za-z_]+[/\\])*deferred\.rs:\d+:\d+:\n",
                r"DeferredGenerator has not been set\n",
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
