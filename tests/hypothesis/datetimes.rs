//! Ported from hypothesis-python/tests/cover/test_datetimes.py
//!
//! timedelta tests and date/datetime range-constraint tests are omitted from
//! this port: `timedeltas()` does not yet exist in hegel-rust (the generator
//! needs a signed multi-component type), and `DateGenerator`/`DateTimeGenerator`
//! do not yet support `min_value`/`max_value` constraints.  Those will be
//! added in a follow-up once the missing features land.
//!
//! Two tests are server-only (`#[cfg(not(feature = "native"))]`) because
//! Hypothesis shrinks dates toward 2000-01-01 while the native engine
//! shrinks toward its min bound (1970-01-01).

use crate::common::utils::{assert_all_examples, find_any, minimal};
use hegel::generators::{self as gs};

fn date_year(s: &str) -> i32 {
    s.split('-').next().unwrap().parse().unwrap()
}

fn date_month(s: &str) -> u32 {
    s.split('-').nth(1).unwrap().parse().unwrap()
}

fn parse_time_parts(s: &str) -> (u32, u32, u32, u32) {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    let hour: u32 = parts[0].parse().unwrap();
    let minute: u32 = parts[1].parse().unwrap();
    let mut frac = parts[2].splitn(2, '.');
    let second: u32 = frac.next().unwrap().parse().unwrap();
    let microsecond: u32 = frac.next().map_or(0, |f| f.parse().unwrap_or(0));
    (hour, minute, second, microsecond)
}

#[cfg(not(feature = "native"))]
fn datetime_parts(s: &str) -> (i32, u32, u32, u32, u32, u32, u32) {
    let mut dt = s.splitn(2, 'T');
    let date_str = dt.next().unwrap();
    let time_str = dt.next().unwrap();
    let mut d = date_str.split('-');
    let year: i32 = d.next().unwrap().parse().unwrap();
    let month: u32 = d.next().unwrap().parse().unwrap();
    let day: u32 = d.next().unwrap().parse().unwrap();
    let (hour, minute, second, microsecond) = parse_time_parts(time_str);
    (year, month, day, hour, minute, second, microsecond)
}

// --- datetime tests ---

#[cfg(not(feature = "native"))]
#[test]
fn test_simplifies_towards_millenium() {
    // Hypothesis shrinks datetimes toward 2000-01-01T00:00:00; the native
    // engine shrinks toward 1970-01-01T00:00:00 (its min bound).
    let d = minimal(gs::datetimes(), |_: &String| true);
    let (year, month, day, hour, minute, second, microsecond) = datetime_parts(&d);
    assert_eq!(year, 2000);
    assert_eq!(month, 1);
    assert_eq!(day, 1);
    assert_eq!(hour, 0);
    assert_eq!(minute, 0);
    assert_eq!(second, 0);
    assert_eq!(microsecond, 0);
}

#[test]
fn test_default_datetimes_are_naive() {
    assert_all_examples(gs::datetimes(), |s: &String| {
        !s.contains('+') && !s.contains('Z')
    });
}

#[test]
fn test_allow_imaginary_is_not_an_error_for_naive_datetimes() {
    // gs::datetimes() always produces naive datetimes; allow_imaginary=False is a no-op
    assert_all_examples(gs::datetimes(), |_: &String| true);
}

// --- date tests ---

#[test]
fn test_can_find_after_the_year_2000() {
    let d = minimal(gs::dates(), |s: &String| date_year(s) > 2000);
    assert_eq!(date_year(&d), 2001);
}

#[cfg(not(feature = "native"))]
#[test]
fn test_can_find_before_the_year_2000() {
    // Hypothesis shrinks toward 2000, so the minimal year < 2000 is 1999.
    // Native engine shrinks toward 1970 (its min bound), giving 1970 instead.
    let d = minimal(gs::dates(), |s: &String| date_year(s) < 2000);
    assert_eq!(date_year(&d), 1999);
}

#[test]
fn test_can_find_each_month() {
    for month in 1u32..=12 {
        find_any(gs::dates(), move |s: &String| date_month(s) == month);
    }
}

// --- time tests ---

#[test]
fn test_can_find_midnight() {
    find_any(gs::times(), |s: &String| {
        let (h, m, sec, _) = parse_time_parts(s);
        h == 0 && m == 0 && sec == 0
    });
}

#[test]
fn test_can_find_non_midnight() {
    let t = minimal(gs::times(), |s: &String| parse_time_parts(s).0 != 0);
    assert_eq!(parse_time_parts(&t).0, 1);
}

#[test]
fn test_can_find_on_the_minute() {
    find_any(gs::times(), |s: &String| parse_time_parts(s).2 == 0);
}

#[test]
fn test_can_find_off_the_minute() {
    find_any(gs::times(), |s: &String| parse_time_parts(s).2 != 0);
}

#[test]
fn test_simplifies_towards_midnight() {
    let t = minimal(gs::times(), |_: &String| true);
    let (hour, minute, second, microsecond) = parse_time_parts(&t);
    assert_eq!(hour, 0);
    assert_eq!(minute, 0);
    assert_eq!(second, 0);
    assert_eq!(microsecond, 0);
}

#[test]
fn test_can_generate_naive_time() {
    find_any(gs::times(), |s: &String| !s.contains('+') && !s.contains('Z'));
}

#[test]
fn test_naive_times_are_naive() {
    assert_all_examples(gs::times(), |s: &String| !s.contains('+') && !s.contains('Z'));
}
