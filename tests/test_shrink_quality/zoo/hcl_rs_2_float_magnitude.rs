//! From hegel-zoo `rust/hcl-rs`, bug hcl-rs/2, test `props::prop_number_from_f64_roundtrips_value`.
//!
//! `Number::from_f64` turns a whole float into an `i64` with `as`, which saturates: every
//! `f > 2^63` that is whole becomes `i64::MAX` and every `f < -2^63` becomes `i64::MIN`, so
//! `as_f64()` no longer equals `f` (`2^63` itself survives: `i64::MAX as f64 == 2^63`). The
//! generator is `one_of!(floats (finite), sampled_from([1e19, -1e19, 9.3e18, 1e300, -1e300]))`;
//! the shortlex ideal is branch 0 with the smallest failing float, `nextup(2^63)`.
//!
//! Every table entry fails, so a seed whose first failure is in branch 1 reduces to `[1, 0]` at
//! once and is then stuck: the ideal needs the branch lowered with a non-minimal continuation
//! of a different choice type, which only a random continuation can supply, and every float
//! above `2^63` fails, so once one is found the bisection has nothing in its way.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug)]
#[allow(dead_code)]
struct Case {
    branch: u8,
    f: f64,
}

fn draw(tc: &TestCase) -> Case {
    let branch = tc.draw_silent(gs::integers::<u8>().min_value(0).max_value(1));
    let f = if branch == 0 {
        tc.draw_silent(gs::floats::<f64>().allow_nan(false).allow_infinity(false))
    } else {
        tc.draw_silent(gs::sampled_from(vec![
            1e19f64, -1e19, 9.3e18, 1e300, -1e300,
        ]))
    };
    Case { branch, f }
}

fn from_f64_loses_the_value(c: &Case) -> bool {
    let f = c.f;
    let back = if f.fract() == 0.0 {
        (f as i64) as f64
    } else {
        f
    };
    back != f
}

fn ideal() -> Case {
    Case {
        branch: 0,
        f: 9.223372036854778e18,
    }
}

#[test]
fn the_ideal_is_the_smallest() {
    assert!(from_f64_loses_the_value(&ideal()));
    assert!(!from_f64_loses_the_value(&Case {
        branch: 0,
        f: 9.223372036854776e18,
    }));
    assert_eq!(
        ideal().f,
        f64::from_bits((9.223372036854776e18f64).to_bits() + 1)
    );
}

#[test]
fn large_floats_shrink_to_the_smallest_lossy_one() {
    assert_shrinks_to(&ideal(), 20, 100, draw, from_f64_loses_the_value);
}
