use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use chrono::TimeDelta;
use hegel::extras::chrono as chrono_gs;
use hegel::generators::{self as gs, Generator};

// ---------------------------------------------------------------------------
// Days
// ---------------------------------------------------------------------------

#[test]
fn test_days_default_generator() {
    check_can_generate_examples(gs::default::<chrono::Days>());
}

// ---------------------------------------------------------------------------
// Months (count)
// ---------------------------------------------------------------------------

#[test]
fn test_months_count_default_generator() {
    check_can_generate_examples(gs::default::<chrono::Months>());
}

// ---------------------------------------------------------------------------
// TimeDelta
// ---------------------------------------------------------------------------

#[test]
fn test_time_deltas_default() {
    check_can_generate_examples(chrono_gs::time_deltas());
}

#[test]
fn test_time_deltas_min_value() {
    let min = TimeDelta::zero();
    assert_all_examples(chrono_gs::time_deltas().min_value(min), move |d| *d >= min);
}

#[test]
fn test_time_deltas_max_value() {
    let max = TimeDelta::seconds(60);
    assert_all_examples(chrono_gs::time_deltas().max_value(max), move |d| *d <= max);
}

#[test]
fn test_time_deltas_in_vec() {
    let max = TimeDelta::seconds(60);
    assert_all_examples(
        gs::vecs(chrono_gs::time_deltas().max_value(max)).max_size(5),
        move |v| v.iter().all(|d| *d <= max),
    );
}

#[test]
fn test_time_delta_default_generator() {
    check_can_generate_examples(gs::default::<TimeDelta>());
}

#[hegel::test]
fn test_time_deltas_property(tc: hegel::TestCase) {
    let lo = tc.draw(
        gs::integers::<i64>()
            .min_value(-1_000_000)
            .max_value(1_000_000),
    );
    let hi = tc.draw(gs::integers::<i64>().min_value(lo).max_value(2_000_000));
    let min = TimeDelta::nanoseconds(lo);
    let max = TimeDelta::nanoseconds(hi);
    let v = tc.draw(chrono_gs::time_deltas().min_value(min).max_value(max));
    assert!(v >= min && v <= max);
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_time_deltas_min_greater_than_max() {
    let g = chrono_gs::time_deltas()
        .min_value(TimeDelta::seconds(10))
        .max_value(TimeDelta::seconds(5));
    g.as_basic();
}

#[test]
fn test_time_deltas_full_range_bounds() {
    // The full TimeDelta range exceeds i64 nanoseconds; the generator must
    // still accept TimeDelta::MIN and TimeDelta::MAX as bounds.
    check_can_generate_examples(
        chrono_gs::time_deltas()
            .min_value(TimeDelta::MIN)
            .max_value(TimeDelta::MAX),
    );
}

#[test]
fn test_time_deltas_beyond_i64_nanos() {
    // 1000 years is well past i64-nanosecond range (~292 years) but well
    // within TimeDelta::MAX. Both bound and drawn values must round-trip.
    let one_kyear = TimeDelta::seconds(1_000 * 365 * 86_400);
    assert_all_examples(
        chrono_gs::time_deltas()
            .min_value(one_kyear)
            .max_value(one_kyear * 2),
        move |d| *d >= one_kyear && *d <= one_kyear * 2,
    );
}
