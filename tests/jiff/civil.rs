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
