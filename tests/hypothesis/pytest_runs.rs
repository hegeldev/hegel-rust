//! Ported from resources/hypothesis/hypothesis-python/tests/pytest/test_runs.py.

use hegel::generators::{self as gs};
use hegel::{Hegel, Settings};

#[test]
fn test_ints_are_ints() {
    Hegel::new(|tc| {
        tc.draw(gs::integers::<i64>());
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_ints_are_floats() {
    // @fails in the original: `isinstance(x, float)` is always False for ints.
    // Rust's type system makes the isinstance check a no-op, so the faithful
    // port is a guaranteed-failing property; we verify Hegel reports failure.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            tc.draw(gs::integers::<i64>());
            panic!("x is not a float");
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }));
    assert!(result.is_err());
}
