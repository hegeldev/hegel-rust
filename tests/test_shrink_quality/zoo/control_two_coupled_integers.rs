//! Control case: sprintf/13 reduced to its two essential draws, which the shrinker handles.
//!
//! `width ∈ [1, 30]` and `value ∈ [-1000, 1000]`; the property fails whenever
//! `width > len(value.to_string())`. `value` is irrelevant given a large enough width, but the
//! two are coupled: lowering `width` alone can leave the failing set. Shortlex ideal
//! `width = 2, value = 0`. With no shape change in the way, `binary_search_integer_towards_zero`
//! zeroes `value` and `width` follows; `sprintf_13_irrelevant_argument` shows what happens when
//! the same coupling sits behind a shape change.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug)]
struct Case {
    width: i64,
    value: i64,
}

fn draw(tc: &TestCase) -> Case {
    Case {
        width: tc.draw_silent(gs::integers::<i64>().min_value(1).max_value(30)),
        value: tc.draw_silent(gs::integers::<i64>().min_value(-1000).max_value(1000)),
    }
}

fn padding_missing(c: &Case) -> bool {
    c.width > c.value.to_string().len() as i64
}

#[test]
fn the_ideal_does_fail() {
    assert!(padding_missing(&Case { width: 2, value: 0 }));
    assert!(!padding_missing(&Case { width: 1, value: 0 }));
}

#[test]
fn coupled_integers_shrink_to_two_and_zero() {
    assert_shrinks_to(&Case { width: 2, value: 0 }, 30, 100, draw, padding_missing);
}
