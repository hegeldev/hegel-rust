//! Distilled from bytesize/1 and humansize/2: a sparse failing set that binary search passes
//! straight through.
//!
//! `m ∈ [0, 10^9]`; the property fails iff `m > 0 && m % 1000 == 0`. Shortlex ideal `1000`. The
//! failing set is a lattice, so bisecting from the first failing draw (a round boundary value
//! such as `10^9`) halves while the half is still a multiple of 1000 and stops at the first half
//! that is not (`10^9 / 2^6 = 15_625_000`). Dividing the distance by 5 and 10 carries on from
//! there, but a start such as `976_000_000` bottoms out at `61_000`, where the remaining factor
//! is a prime no divisor step removes; dropping every digit but the trailing zeros takes it to
//! `1000` in one move. The dense periodic set `m % 1000 ≥ 500` is the control: once the search
//! is inside `[0, 1000)` the set is an interval. A human would write `1000`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> i64 {
    tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(1_000_000_000))
}

fn is_positive_multiple_of_1000(m: &i64) -> bool {
    *m > 0 && m % 1000 == 0
}

fn upper_half_of_period(m: &i64) -> bool {
    m % 1000 >= 500
}

#[test]
fn the_ideal_is_the_smallest() {
    assert!(is_positive_multiple_of_1000(&1000));
    assert!((1..1000).all(|m| !is_positive_multiple_of_1000(&m)));
    assert!(is_positive_multiple_of_1000(&15_625_000));
    assert!(!is_positive_multiple_of_1000(&7_812_500));
}

#[test]
fn sparse_multiples_shrink_to_the_first_one() {
    assert_shrinks_to(&1000, 30, 1000, draw, is_positive_multiple_of_1000);
}

#[test]
fn periodic_set_is_handled() {
    assert_shrinks_to(&500, 30, 1000, draw, upper_half_of_period);
}
