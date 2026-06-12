//! A failing run's closing panic is the *test's own* panic, re-raised: the
//! payload that unwinds out of `Hegel::run` is the one the test body
//! panicked with on the final replay, not a synthetic panic of Hegel's.
//! The report (draws, diagnostic, reproducer) has already been printed at
//! the catch site, and the re-raise skips the panic hook, so nothing prints
//! twice.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::project::TempRustProject;
use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase, Verbosity};

fn failing_run_payload(test_fn: impl FnMut(TestCase) + 'static) -> Box<dyn std::any::Any + Send> {
    catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(test_fn)
            .settings(
                Settings::new()
                    .database(None)
                    .derandomize(true)
                    .test_cases(10)
                    .verbosity(Verbosity::Quiet),
            )
            .run();
    }))
    .expect_err("the property must fail")
}

#[test]
fn failing_run_reraises_the_tests_own_panic_message() {
    let payload = failing_run_payload(|tc| {
        tc.draw(gs::booleans());
        panic!("the actual bug");
    });
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or_default();
    assert_eq!(msg, "the actual bug");
}

/// A non-string payload from `panic_any` survives the run intact: the
/// report shows "Unknown panic" (there is no message to render), but the
/// payload that reaches the caller is the original value, still
/// downcastable to its real type.
struct CustomPayload(u64);

#[test]
fn failing_run_preserves_a_custom_panic_payload() {
    let payload = failing_run_payload(|tc| {
        tc.draw(gs::booleans());
        std::panic::panic_any(CustomPayload(7));
    });
    let custom = payload
        .downcast_ref::<CustomPayload>()
        .expect("the original payload type must survive the run");
    assert_eq!(custom.0, 7);
}

const FAILING_BINARY_CODE: &str = r#"
use hegel::{Hegel, Settings};
use hegel::generators as gs;

fn main() {
    Hegel::new(|tc| {
        let _: bool = tc.draw(gs::booleans());
        panic!("intentional failure");
    })
    .settings(Settings::new().database(None).derandomize(true))
    .run();
}
"#;

/// The failure message appears exactly once on stderr: in the diagnostic
/// printed at the catch site. The closing re-raise skips the panic hook, so
/// there is no second `thread 'main' panicked at ...` block and no
/// `Property test failed:` framing.
#[test]
fn failing_binary_prints_the_failure_exactly_once() {
    let output = TempRustProject::new()
        .main_file(FAILING_BINARY_CODE)
        .invoke()
        .env_remove("RUST_BACKTRACE")
        .expect_failure("intentional failure")
        .cargo_run(&[]);
    assert_eq!(
        output.stderr.matches("intentional failure").count(),
        1,
        "the failure must print exactly once, got:\n{}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Property test failed"),
        "no synthetic framing expected, got:\n{}",
        output.stderr
    );
    assert_eq!(
        output.stderr.matches("panicked at").count(),
        1,
        "only the catch-site diagnostic should mention the panic, got:\n{}",
        output.stderr
    );
}
