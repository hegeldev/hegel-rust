//! Ported from hypothesis-python/tests/cover/test_control.py
//!
//! Individually-skipped tests:
//! - test_cannot_cleanup_with_no_context — `cleanup()` is Hypothesis public
//!   API with no hegel-rust counterpart.
//! - test_cannot_event_with_no_context — `event()` is Hypothesis public API
//!   with no hegel-rust counterpart.
//! - test_cleanup_executes_on_leaving_build_context — `cleanup()` /
//!   `BuildContext` with no hegel-rust counterpart.
//! - test_can_nest_build_context — `BuildContext` context-manager shape
//!   has no hegel-rust counterpart (test context is a thread-local flag,
//!   not an openable/nestable object).
//! - test_does_not_suppress_exceptions — `BuildContext` context-manager
//!   with no hegel-rust counterpart.
//! - test_suppresses_exceptions_in_teardown — `BuildContext` + `cleanup()`
//!   with no hegel-rust counterpart.
//! - test_runs_multiple_cleanup_with_teardown — `BuildContext` + `cleanup()`
//!   + `ExceptionGroup` with no hegel-rust counterpart.
//! - test_raises_error_if_cleanup_fails_but_block_does_not — `cleanup()`
//!   with no hegel-rust counterpart.
//! - test_raises_if_note_out_of_context — `note()` is a standalone function
//!   in Hypothesis; in hegel-rust it is `TestCase::note`, so calling it
//!   outside a test context is prevented by the type system.
//! - test_deprecation_warning_if_assume_out_of_context — standalone
//!   `assume()` doesn't exist in hegel-rust (it's `TestCase::assume`).
//! - test_deprecation_warning_if_reject_out_of_context — standalone
//!   `reject()` doesn't exist in hegel-rust (it's `TestCase::reject`).
//! - test_raises_if_current_build_context_out_of_context —
//!   `current_build_context()` has no hegel-rust counterpart.
//! - test_current_build_context_is_current — `current_build_context()` /
//!   `BuildContext` with no hegel-rust counterpart.
//! - test_prints_all_notes_in_verbose_mode — hegel-rust's `tc.note()` is
//!   verbosity-independent and only prints on the final failing replay
//!   (see the individually-skipped `test_reporting.py` tests in
//!   SKIPPED.md); the original asserts `note` output during every attempt
//!   under `Verbosity::debug`.
//! - test_note_pretty_prints — uses `hypothesis.reporting.with_reporter`
//!   to redirect reports into a list; hegel-rust has no reporter-override
//!   public API.
//! - test_can_convert_non_weakref_types_to_event_strings — internal
//!   `_event_to_string` helper and Python weak-reference semantics with
//!   no Rust counterpart.

use hegel::TestCase;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_not_currently_in_hypothesis() {
    assert!(!hegel::currently_in_test_context());
}

#[test]
fn test_currently_in_hypothesis() {
    Hegel::new(|tc: TestCase| {
        let _: i64 = tc.draw(gs::integers());
        assert!(hegel::currently_in_test_context());
    })
    .settings(Settings::new().test_cases(10).database(None))
    .run();
}

struct ContextMachine;

#[hegel::state_machine]
impl ContextMachine {
    #[rule]
    fn step(&mut self, _tc: TestCase) {
        assert!(hegel::currently_in_test_context());
    }
}

#[test]
fn test_currently_in_stateful_test() {
    Hegel::new(|tc: TestCase| {
        let m = ContextMachine;
        hegel::stateful::run(m, tc);
    })
    .settings(Settings::new().test_cases(10).database(None))
    .run();
}
