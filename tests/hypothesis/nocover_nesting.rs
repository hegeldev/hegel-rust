//! Ported from hypothesis-python/tests/nocover/test_nesting.py
//!
//! The upstream pins down that `@given` works when called from inside
//! another `@given` body. In hegel-rust there's no decorator-based
//! dispatch and no `HealthCheck.nested_given`, so the
//! `suppress_health_check=[HealthCheck.nested_given]` and `phases=no_shrink`
//! scaffolding from the original becomes irrelevant; only the nested
//! `Hegel::new(...).run()` core carries over.

use crate::common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Hegel, HealthCheck, Settings};

#[test]
fn test_nesting_1() {
    // Each outer test case runs an *entire* inner `Hegel::new(...).run()` to
    // exhaustion before yielding back. With 100 inner cases and the system
    // under concurrent load (other test binaries running in the same
    // `cargo test`), one outer iteration can comfortably exceed the
    // 200 ms / case TooSlow threshold — that's the point of this test, not
    // a bug. Suppress the check on both runners. Mirrors the upstream
    // Python `suppress_health_check=[HealthCheck.nested_given]` (Hegel
    // doesn't have a `nested_given` variant, so TooSlow + FilterTooMuch is
    // the equivalent set: nested_given covered both shapes upstream).
    let outer_settings = Settings::new()
        .test_cases(5)
        .database(None)
        .suppress_health_check([HealthCheck::TooSlow, HealthCheck::FilterTooMuch]);
    Hegel::new(|tc| {
        let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        expect_panic(
            move || {
                Hegel::new(move |tc_inner| {
                    let y: i64 = tc_inner.draw(gs::integers::<i64>());
                    if y >= x {
                        panic!("inner_panic");
                    }
                })
                .settings(
                    Settings::new()
                        .test_cases(100)
                        .database(None)
                        .suppress_health_check([
                            HealthCheck::TooSlow,
                            HealthCheck::FilterTooMuch,
                        ]),
                )
                .run();
            },
            "inner_panic",
        );
    })
    .settings(outer_settings)
    .run();
}
