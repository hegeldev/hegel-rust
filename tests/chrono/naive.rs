use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use hegel::extras::chrono as chrono_gs;
use hegel::generators::{self as gs, Generator};

// ---------------------------------------------------------------------------
// NaiveDate
// ---------------------------------------------------------------------------

#[test]
fn test_naive_dates_default() {
    check_can_generate_examples(chrono_gs::naive_dates());
}

#[test]
fn test_naive_dates_min_value() {
    let min = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    assert_all_examples(chrono_gs::naive_dates().min_value(min), move |d| *d >= min);
}

#[test]
fn test_naive_dates_max_value() {
    let max = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
    assert_all_examples(chrono_gs::naive_dates().max_value(max), move |d| *d <= max);
}

#[test]
fn test_naive_dates_in_vec() {
    let min = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let max = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
    assert_all_examples(
        gs::vecs(chrono_gs::naive_dates().min_value(min).max_value(max)).max_size(5),
        move |v| v.iter().all(|d| *d >= min && *d <= max),
    );
}

#[test]
fn test_naive_date_default_generator() {
    check_can_generate_examples(gs::default::<NaiveDate>());
}

#[hegel::test]
fn test_naive_dates_property(tc: hegel::TestCase) {
    let lo_days = tc.draw(gs::integers::<i32>().min_value(-100_000).max_value(100_000));
    let hi_days = tc.draw(gs::integers::<i32>().min_value(lo_days).max_value(200_000));
    let min = NaiveDate::from_num_days_from_ce_opt(lo_days).unwrap();
    let max = NaiveDate::from_num_days_from_ce_opt(hi_days).unwrap();
    let v = tc.draw(chrono_gs::naive_dates().min_value(min).max_value(max));
    assert!(v >= min && v <= max);
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_naive_dates_min_greater_than_max() {
    let g = chrono_gs::naive_dates()
        .min_value(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap())
        .max_value(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
    g.as_basic();
}

// ---------------------------------------------------------------------------
// NaiveTime
// ---------------------------------------------------------------------------

#[test]
fn test_naive_times_default() {
    assert_all_examples(chrono_gs::naive_times(), |t| t.nanosecond() < 1_000_000_000);
}

#[test]
fn test_naive_times_min_value() {
    let min = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
    assert_all_examples(chrono_gs::naive_times().min_value(min), move |t| *t >= min);
}

#[test]
fn test_naive_times_max_value() {
    let max = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
    assert_all_examples(chrono_gs::naive_times().max_value(max), move |t| *t <= max);
}

#[test]
fn test_naive_times_in_vec() {
    let max = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
    assert_all_examples(
        gs::vecs(chrono_gs::naive_times().max_value(max)).max_size(5),
        move |v| v.iter().all(|t| *t <= max),
    );
}

#[test]
fn test_naive_time_default_generator() {
    check_can_generate_examples(gs::default::<NaiveTime>());
}

#[hegel::test]
fn test_naive_times_property(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<u32>().min_value(0).max_value(86_399));
    let hi = tc.draw(gs::integers::<u32>().min_value(lo).max_value(86_399));
    let min = NaiveTime::from_num_seconds_from_midnight_opt(lo, 0).unwrap();
    let max = NaiveTime::from_num_seconds_from_midnight_opt(hi, 999_999_999).unwrap();
    let v = tc.draw(chrono_gs::naive_times().min_value(min).max_value(max));
    assert!(v >= min && v <= max);
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_naive_times_min_greater_than_max() {
    let g = chrono_gs::naive_times()
        .min_value(NaiveTime::from_hms_opt(20, 0, 0).unwrap())
        .max_value(NaiveTime::from_hms_opt(8, 0, 0).unwrap());
    g.as_basic();
}

#[test]
fn test_naive_times_preserves_leap_second_bound() {
    // chrono encodes leap seconds by letting nanosecond() reach into [1e9, 2e9).
    // Bounds passed to naive_times() must round-trip losslessly through the
    // internal i64 representation rather than silently clamping.
    let leap = NaiveTime::from_hms_nano_opt(23, 59, 59, 1_500_000_000).unwrap();
    assert_eq!(leap.nanosecond(), 1_500_000_000);
    assert_all_examples(
        chrono_gs::naive_times().min_value(leap).max_value(leap),
        move |t| *t == leap,
    );
}

// ---------------------------------------------------------------------------
// NaiveDateTime
// ---------------------------------------------------------------------------

#[test]
fn test_naive_datetimes_default() {
    check_can_generate_examples(chrono_gs::naive_datetimes());
}

#[test]
fn test_naive_datetimes_min_value() {
    let min = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    );
    assert_all_examples(chrono_gs::naive_datetimes().min_value(min), move |dt| {
        *dt >= min
    });
}

#[test]
fn test_naive_datetimes_max_value() {
    let max = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap(),
    );
    assert_all_examples(chrono_gs::naive_datetimes().max_value(max), move |dt| {
        *dt <= max
    });
}

#[test]
fn test_naive_datetimes_in_vec() {
    let min = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    );
    let max = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap(),
    );
    assert_all_examples(
        gs::vecs(chrono_gs::naive_datetimes().min_value(min).max_value(max)).max_size(5),
        move |v| v.iter().all(|dt| *dt >= min && *dt <= max),
    );
}

#[test]
fn test_naive_datetime_default_generator() {
    check_can_generate_examples(gs::default::<NaiveDateTime>());
}

// ---------------------------------------------------------------------------
// IsoWeek
// ---------------------------------------------------------------------------

#[test]
fn test_iso_weeks_default() {
    assert_all_examples(gs::default::<chrono::IsoWeek>(), |w| {
        (1..=53).contains(&w.week())
    });
}

#[test]
fn test_iso_weeks_in_vec() {
    assert_all_examples(
        gs::vecs(gs::default::<chrono::IsoWeek>()).max_size(5),
        |v| v.iter().all(|w| (1..=53).contains(&w.week())),
    );
}

#[test]
fn test_iso_week_default_generator() {
    check_can_generate_examples(gs::default::<chrono::IsoWeek>());
}

// ---------------------------------------------------------------------------
// NaiveWeek
// ---------------------------------------------------------------------------

#[test]
fn test_naive_weeks_default() {
    check_can_generate_examples(chrono_gs::naive_weeks());
}

#[test]
fn test_naive_weeks_start() {
    // Override the start-day strategy to a constant Sunday.
    assert_all_examples(
        chrono_gs::naive_weeks().weekday_starts(gs::just(Weekday::Sun)),
        |w| match w.checked_first_day() {
            Some(d) => d.weekday() == Weekday::Sun,
            None => true,
        },
    );
}

#[test]
fn test_naive_weeks_min_date() {
    let min = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
    assert_all_examples(chrono_gs::naive_weeks().min_date(min), move |w| {
        match w.checked_last_day() {
            Some(d) => d >= min,
            None => true,
        }
    });
}

#[test]
fn test_naive_weeks_max_date() {
    let max = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
    assert_all_examples(chrono_gs::naive_weeks().max_date(max), move |w| {
        match w.checked_first_day() {
            Some(d) => d <= max,
            None => true,
        }
    });
}

#[test]
fn test_naive_weeks_in_vec() {
    assert_all_examples(
        gs::vecs(chrono_gs::naive_weeks().weekday_starts(gs::just(Weekday::Mon))).max_size(5),
        |v| {
            v.iter().all(|w| match w.checked_first_day() {
                Some(d) => d.weekday() == Weekday::Mon,
                None => true,
            })
        },
    );
}

#[test]
fn test_naive_week_default_generator() {
    check_can_generate_examples(gs::default::<chrono::NaiveWeek>());
}
