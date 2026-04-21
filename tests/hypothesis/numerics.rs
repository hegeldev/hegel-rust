//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_numerics.py.
//!
//! The upstream file is dominated by tests targeting the `decimals()` and
//! `fractions()` strategies, which generate Python stdlib types
//! (`decimal.Decimal`, `fractions.Fraction`) with no hegel-rust counterpart.
//! Those tests are individually skipped.
//!
//! Individually-skipped tests:
//!
//! - `test_fuzz_fractions_bounds`, `test_fraction_addition_is_well_behaved` —
//!   use the `fractions()` strategy (Python `fractions.Fraction`, no Rust
//!   counterpart).
//! - `test_fuzz_decimals_bounds`, `test_all_decimals_can_be_exact_floats`,
//!   `test_decimals_include_nan`, `test_decimals_include_inf`,
//!   `test_decimals_can_disallow_nan`, `test_decimals_can_disallow_inf`,
//!   `test_decimals_have_correct_places`, `test_works_with_few_values`,
//!   `test_issue_725_regression`, `test_issue_739_regression`,
//!   `test_consistent_decimal_error`, `test_minimal_nonfinite_decimal_is_inf`,
//!   `test_decimals_warns_for_inexact_numeric_bounds` — use the `decimals()`
//!   strategy (Python `decimal.Decimal`, no Rust counterpart).
//! - `test_floats_message` (all four parametrize rows) — asserts on the exact
//!   `InvalidArgument` message emitted by Hypothesis's float validator for
//!   infinite bounds combined with `allow_infinity=False`. hegel-rust's float
//!   generator always fills in a default max_value of `f64::MAX` when
//!   `allow_infinity=False` and no explicit `max_value` is set, which masks
//!   the upstream error with a different "no floats between …" message.
//!
//! `test_fuzz_floats_bounds` parameterised over `width in [64, 32, 16]` in
//! Python; hegel-rust's float generator has no `width` parameter (width is
//! implicit in the generator's type), so this port runs the f64 variant only.
//! The port also filters out bounds of ±inf via `tc.assume`: Hypothesis
//! infers `allow_infinity=True` when a bound is infinite, whereas
//! hegel-rust's float generator keys `allow_infinity` purely off whether the
//! bounds are set, and would reject the schema before the test could run.

use hegel::generators as gs;
use hegel::{HealthCheck, Hegel, Settings};

#[test]
fn test_fuzz_floats_bounds() {
    Hegel::new(|tc| {
        let (mut low, mut high): (Option<f64>, Option<f64>) = tc.draw(gs::tuples!(
            gs::optional(gs::floats::<f64>().allow_nan(false)),
            gs::optional(gs::floats::<f64>().allow_nan(false)),
        ));
        if let (Some(l), Some(h)) = (low, high) {
            if l > h {
                std::mem::swap(&mut low, &mut high);
            }
        }
        // See module docstring: hegel-rust's float generator can't express
        // `floats(min_value=inf, ...)` the way Hypothesis can.
        tc.assume(!low.is_some_and(|l| l.is_infinite()));
        tc.assume(!high.is_some_and(|h| h.is_infinite()));

        let exmin = low.is_some() && tc.draw(gs::booleans());
        let exmax = high.is_some() && tc.draw(gs::booleans());

        if let (Some(l), Some(h)) = (low, high) {
            let lo = if exmin { l.next_up() } else { l };
            let hi = if exmax { h.next_down() } else { h };
            tc.assume(lo <= hi);
            if lo == 0.0 && hi == 0.0 {
                tc.assume(!exmin && !exmax && (1.0_f64).copysign(lo) <= (1.0_f64).copysign(hi));
            }
        }

        let mut s = gs::floats::<f64>();
        if let Some(l) = low {
            s = s.min_value(l);
        }
        if let Some(h) = high {
            s = s.max_value(h);
        }
        s = s.exclude_min(exmin).exclude_max(exmax);
        let val: f64 = tc.draw(s);
        tc.assume(val != 0.0); // positive/negative zero is an issue

        if let Some(l) = low {
            assert!(l <= val);
        }
        if let Some(h) = high {
            assert!(val <= h);
        }
        if exmin {
            assert_ne!(low.unwrap(), val);
        }
        if exmax {
            assert_ne!(high.unwrap(), val);
        }
    })
    .settings(
        Settings::new()
            .suppress_health_check(HealthCheck::all())
            .database(None),
    )
    .run();
}
