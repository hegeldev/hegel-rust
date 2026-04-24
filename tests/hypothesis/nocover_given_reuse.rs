//! Ported from hypothesis-python/tests/nocover/test_given_reuse.py.
//!
//! The Python original tests that a `@given(st.booleans())` decorator value
//! can be re-bound and re-applied to multiple test functions with different
//! argument names, and that failures in one application don't bleed into
//! another. hegel-rust has no `@given` decorator; the analog is sharing a
//! generator value across multiple `Hegel::new(...)` invocations.

use hegel::generators::{self as gs};
use hegel::{Hegel, Settings};

#[test]
fn test_has_an_arg_named_x() {
    let g = gs::booleans();
    Hegel::new(|tc| {
        let _x: bool = tc.draw(&g);
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_has_an_arg_named_y() {
    let g = gs::booleans();
    Hegel::new(|tc| {
        let _y: bool = tc.draw(&g);
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_fail_independently() {
    let g = gs::text();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            let _z: String = tc.draw(&g);
            panic!("AssertionError");
        })
        .settings(Settings::new().database(None))
        .run();
    }));
    assert!(result.is_err());

    Hegel::new(|tc| {
        let _z: String = tc.draw(&g);
    })
    .settings(Settings::new().database(None))
    .run();
}
