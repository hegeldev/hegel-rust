//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_given_error_conditions.py.
//!
//! Individually-skipped tests (see SKIPPED.md):
//!
//! - `test_raises_unsatisfiable_if_passed_explicit_nothing` — uses `nothing()`,
//!   the empty-generator strategy; hegel-rust has no `gs::nothing()` public API.
//! - `test_error_if_has_no_hints`, `test_error_if_infer_all_and_has_no_hints`,
//!   `test_error_if_infer_is_posarg`, `test_error_if_infer_is_posarg_mixed_with_kwarg`
//!   — exercise Python's `@given(a=...)` / `@given(...)` ellipsis syntax for
//!   type-hint-based strategy inference. `#[hegel::test]` takes generators
//!   directly, so this inference mechanism has no Rust counterpart.
//! - `test_given_twice_is_an_error` — stacks two `@given` decorators on one
//!   function; `#[hegel::test]` doesn't compose that way.
//! - `test_given_is_not_a_class_decorator` — applies `@given` to a Python
//!   class; Rust has no analogous class/macro composition.
//! - `test_specific_error_for_coroutine_functions` — asserts a specific error
//!   for Python `async def` tests; hegel-rust has no async-test dispatch.
//! - `test_suggests_at_settings_if_extra_kwarg_matches_setting_name` —
//!   inspects `@given` kwarg handling against Python setting names. hegel-rust
//!   uses the `.settings(Settings::new()...)` builder rather than kwargs on
//!   the test macro.

#[cfg(feature = "native")]
use crate::common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase};

// Port of `test_raises_unsatisfiable_if_all_false_in_finite_set`. In native
// mode, a test that always rejects trips the `FilterTooMuch` health check
// (hegel-rust's analog of Hypothesis's `Unsatisfiable`). In server mode, the
// runner silently passes on all-rejected runs, so this assertion is
// native-only.
#[cfg(feature = "native")]
#[test]
fn test_raises_unsatisfiable_if_all_false_in_finite_set() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                tc.draw(gs::booleans());
                tc.reject();
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "FilterTooMuch",
    );
}

#[test]
fn test_does_not_raise_unsatisfiable_if_some_false_in_finite_set() {
    Hegel::new(|tc: TestCase| {
        let x: bool = tc.draw(gs::booleans());
        tc.assume(x);
    })
    .settings(Settings::new().database(None))
    .run();
}
