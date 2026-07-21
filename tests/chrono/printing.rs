use crate::common::utils::printed_draw_lines;
use hegel::extras::chrono as chrono_gs;

#[test]
fn every_chrono_generator_prints_its_drawn_value() {
    printed_draw_lines(chrono_gs::weekday_sets());
    printed_draw_lines(chrono_gs::fixed_offsets());
    printed_draw_lines(chrono_gs::time_deltas());
    printed_draw_lines(chrono_gs::naive_dates());
    printed_draw_lines(chrono_gs::naive_times());
    printed_draw_lines(chrono_gs::naive_datetimes());
    printed_draw_lines(chrono_gs::naive_weeks());
    printed_draw_lines(chrono_gs::datetimes());
}

fn render<T: hegel::PrettyPrintable>(value: &T) -> String {
    let mut doc = hegel::Document::new();
    value.pretty_print(doc.printer());
    doc.finish()
}

#[test]
fn chrono_values_print_as_constructor_expressions() {
    use chrono::{FixedOffset, NaiveDate, TimeDelta, TimeZone, Weekday, WeekdaySet};

    let date = NaiveDate::from_ymd_opt(2020, 2, 29).unwrap();
    assert_eq!(
        render(&date),
        "NaiveDate::from_ymd_opt(2020, 2, 29).unwrap()"
    );
    let time = chrono::NaiveTime::from_hms_nano_opt(1, 2, 3, 40).unwrap();
    assert_eq!(
        render(&time),
        "NaiveTime::from_hms_nano_opt(1, 2, 3, 40).unwrap()"
    );
    assert_eq!(
        render(&date.and_time(time)),
        "NaiveDate::from_ymd_opt(2020, 2, 29).unwrap().and_hms_nano_opt(1, 2, 3, 40).unwrap()"
    );
    assert_eq!(
        render(&FixedOffset::east_opt(3600).unwrap()),
        "FixedOffset::east_opt(3600).unwrap()"
    );
    assert_eq!(
        render(&TimeDelta::milliseconds(-1500)),
        "TimeDelta::new(-2, 500000000).unwrap()"
    );
    assert_eq!(render(&Weekday::Mon), "Weekday::Mon");
    assert_eq!(
        render(&WeekdaySet::from_array([Weekday::Mon, Weekday::Fri])),
        "WeekdaySet::from_array([Weekday::Mon, Weekday::Fri])"
    );
    let week = NaiveDate::from_ymd_opt(2024, 6, 5)
        .unwrap()
        .week(Weekday::Mon);
    assert_eq!(
        render(&week),
        "NaiveDate::from_ymd_opt(2024, 6, 3).unwrap().week(Weekday::Mon)"
    );
    let datetime = FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2020, 1, 1, 0, 0, 0)
        .unwrap();
    assert_eq!(
        render(&datetime),
        "DateTime::parse_from_rfc3339(\"2020-01-01T00:00:00+00:00\").unwrap()"
    );
}
