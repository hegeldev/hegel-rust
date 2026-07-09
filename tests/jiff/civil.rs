use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::extras::jiff as jiff_gs;
use hegel::generators as gs;
use jiff::civil::{Date, DateTime, ISOWeekDate, Time};

#[test]
fn test_jiff_dates_default() {
    check_can_generate_examples(jiff_gs::dates());
}

#[test]
fn test_jiff_dates_year_in_range() {
    assert_all_examples(jiff_gs::dates(), |d| (1..=9999).contains(&d.year()));
}

#[test]
fn test_jiff_dates_in_vec() {
    assert_all_examples(gs::vecs(jiff_gs::dates()).max_size(5), |v| {
        v.iter().all(|d| (1..=9999).contains(&d.year()))
    });
}

#[test]
fn test_jiff_dates_min_value() {
    let min = Date::constant(2024, 6, 1);
    assert_all_examples(jiff_gs::dates().min_value(min), move |d| *d >= min);
}

#[test]
fn test_jiff_dates_max_value() {
    let max = Date::constant(2024, 6, 30);
    assert_all_examples(jiff_gs::dates().max_value(max), move |d| *d <= max);
}

#[test]
fn test_jiff_dates_bounds_can_extend_below_year_one() {
    let min = Date::constant(-9999, 1, 1);
    let max = Date::constant(-1, 12, 31);
    assert_all_examples(jiff_gs::dates().min_value(min).max_value(max), move |d| {
        *d >= min && *d <= max
    });
}

#[hegel::test]
#[should_panic(expected = "max_value < min_value")]
fn test_jiff_dates_min_greater_than_max(tc: hegel::TestCase) {
    tc.draw(
        jiff_gs::dates()
            .min_value(Date::constant(2025, 1, 1))
            .max_value(Date::constant(2024, 1, 1)),
    );
}

#[test]
fn test_jiff_date_default_generator() {
    check_can_generate_examples(gs::default::<Date>());
}

#[test]
fn test_jiff_times_default() {
    check_can_generate_examples(jiff_gs::times());
}

#[test]
fn test_jiff_times_components_in_range() {
    assert_all_examples(jiff_gs::times(), |t| {
        (0..=23).contains(&t.hour())
            && (0..=59).contains(&t.minute())
            && (0..=59).contains(&t.second())
    });
}

#[test]
fn test_jiff_times_in_vec() {
    assert_all_examples(gs::vecs(jiff_gs::times()).max_size(5), |v| {
        v.iter().all(|t| (0..=23).contains(&t.hour()))
    });
}

#[test]
fn test_jiff_times_min_value() {
    let min = Time::constant(12, 30, 0, 0);
    assert_all_examples(jiff_gs::times().min_value(min), move |t| *t >= min);
}

#[test]
fn test_jiff_times_max_value() {
    let max = Time::constant(6, 0, 0, 0);
    assert_all_examples(jiff_gs::times().max_value(max), move |t| *t <= max);
}

/// Generated times are whole microseconds, so a min bound with a
/// sub-microsecond component must round *up* to the next microsecond, not
/// down past the bound.
#[test]
fn test_jiff_times_sub_microsecond_min_bound_rounds_up() {
    let min = Time::constant(1, 2, 3, 500);
    let max = Time::constant(1, 2, 3, 10_000);
    assert_all_examples(jiff_gs::times().min_value(min).max_value(max), move |t| {
        *t >= min && *t <= max && t.subsec_nanosecond() % 1_000 == 0
    });
}

#[hegel::test]
#[should_panic(expected = "max_value < min_value")]
fn test_jiff_times_min_greater_than_max(tc: hegel::TestCase) {
    tc.draw(
        jiff_gs::times()
            .min_value(Time::constant(13, 0, 0, 0))
            .max_value(Time::constant(12, 0, 0, 0)),
    );
}

/// `min_value < max_value`, but no whole microsecond lies between them:
/// a clean usage error rather than an out-of-bounds value.
#[hegel::test]
#[should_panic(expected = "whole microsecond")]
fn test_jiff_times_empty_microsecond_range_is_a_usage_error(tc: hegel::TestCase) {
    tc.draw(
        jiff_gs::times()
            .min_value(Time::constant(12, 0, 0, 100))
            .max_value(Time::constant(12, 0, 0, 900)),
    );
}

#[test]
fn test_jiff_time_default_generator() {
    check_can_generate_examples(gs::default::<Time>());
}

#[test]
fn test_jiff_datetimes_default() {
    check_can_generate_examples(jiff_gs::datetimes());
}

#[test]
fn test_jiff_datetimes_year_in_range() {
    assert_all_examples(jiff_gs::datetimes(), |dt| {
        (-9999..=9999).contains(&dt.year())
    });
}

#[test]
fn test_jiff_datetimes_in_vec() {
    assert_all_examples(gs::vecs(jiff_gs::datetimes()).max_size(5), |v| {
        v.iter().all(|dt| (-9999..=9999).contains(&dt.year()))
    });
}

#[test]
fn test_jiff_datetimes_min_value() {
    let min = jiff::civil::DateTime::constant(2024, 1, 1, 0, 0, 0, 0);
    assert_all_examples(jiff_gs::datetimes().min_value(min), move |dt| *dt >= min);
}

#[test]
fn test_jiff_datetimes_max_value() {
    let max = jiff::civil::DateTime::constant(2024, 12, 31, 23, 59, 59, 999_999_999);
    assert_all_examples(jiff_gs::datetimes().max_value(max), move |dt| *dt <= max);
}

#[hegel::test]
#[should_panic(expected = "max_value < min_value")]
fn test_jiff_datetimes_min_greater_than_max(tc: hegel::TestCase) {
    tc.draw(
        jiff_gs::datetimes()
            .min_value(jiff::civil::DateTime::constant(2025, 1, 1, 0, 0, 0, 0))
            .max_value(jiff::civil::DateTime::constant(2024, 1, 1, 0, 0, 0, 0)),
    );
}

#[test]
fn test_jiff_datetime_default_generator() {
    check_can_generate_examples(gs::default::<DateTime>());
}

#[test]
fn test_jiff_iso_week_dates_default() {
    check_can_generate_examples(gs::default::<ISOWeekDate>());
}

#[test]
fn test_jiff_iso_week_dates_year_in_range() {
    assert_all_examples(gs::default::<ISOWeekDate>(), |w| {
        (0..=9999).contains(&w.year())
    });
}

#[test]
fn test_jiff_iso_week_dates_in_vec() {
    assert_all_examples(gs::vecs(gs::default::<ISOWeekDate>()).max_size(5), |v| {
        v.iter().all(|w| (1..=53).contains(&w.week()))
    });
}
