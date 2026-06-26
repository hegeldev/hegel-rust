use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use chrono::{DateTime, FixedOffset, Utc};
use hegel::extras::chrono as chrono_gs;
use hegel::generators as gs;

#[test]
fn test_datetimes_default() {
    check_can_generate_examples(chrono_gs::datetimes());
}

#[test]
fn test_datetimes_in_vec() {
    check_can_generate_examples(gs::vecs(chrono_gs::datetimes()).max_size(5));
}

#[test]
fn test_datetimes_min_value() {
    let min = DateTime::<Utc>::from_timestamp(0, 0).unwrap().naive_utc();
    assert_all_examples(chrono_gs::datetimes().min_value(min), move |dt| {
        dt.naive_local() >= min
    });
}

#[test]
fn test_datetimes_max_value() {
    let max = DateTime::<Utc>::from_timestamp(2_000_000_000, 0)
        .unwrap()
        .naive_utc();
    assert_all_examples(chrono_gs::datetimes().max_value(max), move |dt| {
        dt.naive_local() <= max
    });
}

#[test]
fn test_datetimes_offset_constrained() {
    let min_off = FixedOffset::east_opt(-3600).unwrap();
    let max_off = FixedOffset::east_opt(3600).unwrap();
    assert_all_examples(
        chrono_gs::datetimes().timezones(
            chrono_gs::fixed_offsets()
                .min_value(min_off)
                .max_value(max_off),
        ),
        move |dt| {
            let secs = dt.offset().local_minus_utc();
            (-3600..=3600).contains(&secs)
        },
    );
}

#[test]
fn test_datetime_fixed_offset_default_generator() {
    check_can_generate_examples(gs::default::<DateTime<FixedOffset>>());
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_datetimes_min_greater_than_max() {
    let g = chrono_gs::datetimes()
        .min_value(
            DateTime::<Utc>::from_timestamp(2_000_000_000, 0)
                .unwrap()
                .naive_utc(),
        )
        .max_value(DateTime::<Utc>::from_timestamp(0, 0).unwrap().naive_utc());
    check_can_generate_examples(g);
}

#[test]
fn test_datetimes_utc_default() {
    check_can_generate_examples(chrono_gs::datetimes().timezones(gs::just(Utc)));
}

#[test]
fn test_datetimes_utc_min_value() {
    let min = DateTime::<Utc>::from_timestamp(0, 0).unwrap();
    assert_all_examples(
        chrono_gs::datetimes()
            .timezones(gs::just(Utc))
            .min_value(min.naive_utc()),
        move |dt| *dt >= min,
    );
}

#[test]
fn test_datetimes_utc_max_value() {
    let max = DateTime::<Utc>::from_timestamp(2_000_000_000, 0).unwrap();
    assert_all_examples(
        chrono_gs::datetimes()
            .timezones(gs::just(Utc))
            .max_value(max.naive_utc()),
        move |dt| *dt <= max,
    );
}

#[test]
fn test_datetimes_utc_in_vec() {
    let max = DateTime::<Utc>::from_timestamp(2_000_000_000, 0).unwrap();
    assert_all_examples(
        gs::vecs(
            chrono_gs::datetimes()
                .timezones(gs::just(Utc))
                .max_value(max.naive_utc()),
        )
        .max_size(5),
        move |v| v.iter().all(|dt| *dt <= max),
    );
}

#[test]
fn test_datetime_utc_default_generator() {
    check_can_generate_examples(gs::default::<DateTime<Utc>>());
}

#[hegel::test]
fn test_datetimes_utc_property(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<i64>().min_value(0).max_value(2_000_000_000));
    let hi = tc.draw(gs::integers::<i64>().min_value(lo).max_value(4_000_000_000));
    let min = DateTime::<Utc>::from_timestamp(lo, 0).unwrap();
    let max = DateTime::<Utc>::from_timestamp(hi, 0).unwrap();
    let v = tc.draw(
        chrono_gs::datetimes()
            .timezones(gs::just(Utc))
            .min_value(min.naive_utc())
            .max_value(max.naive_utc()),
    );
    assert!(v >= min && v <= max);
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_datetimes_utc_min_greater_than_max() {
    let g = chrono_gs::datetimes()
        .timezones(gs::just(Utc))
        .min_value(
            DateTime::<Utc>::from_timestamp(2_000_000_000, 0)
                .unwrap()
                .naive_utc(),
        )
        .max_value(DateTime::<Utc>::from_timestamp(0, 0).unwrap().naive_utc());
    check_can_generate_examples(g);
}

#[test]
fn test_datetimes_full_range_bounds() {
    check_can_generate_examples(
        chrono_gs::datetimes()
            .timezones(gs::just(Utc))
            .min_value(DateTime::<Utc>::MIN_UTC.naive_utc())
            .max_value(DateTime::<Utc>::MAX_UTC.naive_utc()),
    );
}

#[test]
fn test_datetimes_year_3000() {
    let min = "3000-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
    let max = "3001-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
    assert_all_examples(
        chrono_gs::datetimes()
            .timezones(gs::just(Utc))
            .min_value(min.naive_utc())
            .max_value(max.naive_utc()),
        move |dt| *dt >= min && *dt <= max,
    );
}
