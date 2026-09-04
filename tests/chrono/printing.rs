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
    let datetime = FixedOffset::east_opt(3600)
        .unwrap()
        .with_ymd_and_hms(2020, 1, 1, 0, 0, 0)
        .unwrap();
    assert_eq!(
        render(&datetime),
        "NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()\
         .and_hms_nano_opt(0, 0, 0, 0).unwrap()\
         .and_local_timezone(FixedOffset::east_opt(3600).unwrap()).unwrap()"
    );
    let far_future = FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(20_000, 1, 1, 0, 0, 0)
        .unwrap();
    assert_eq!(
        render(&far_future),
        "NaiveDate::from_ymd_opt(20000, 1, 1).unwrap()\
         .and_hms_nano_opt(0, 0, 0, 0).unwrap()\
         .and_local_timezone(FixedOffset::east_opt(0).unwrap()).unwrap()"
    );
}

#[test]
fn month_days_months_and_iso_weeks_print_as_constructor_expressions() {
    use chrono::{Datelike, Days, Month, Months, NaiveDate};
    assert_eq!(render(&Month::January), "Month::January");
    assert_eq!(render(&Days::new(5)), "Days::new(5)");
    assert_eq!(render(&Months::new(7)), "Months::new(7)");
    let week = NaiveDate::from_ymd_opt(2020, 2, 29).unwrap().iso_week();
    assert_eq!(
        render(&week),
        "NaiveDate::from_isoywd_opt(2020, 9, Weekday::Mon).unwrap().iso_week()"
    );
}

#[test]
fn every_chrono_default_generator_is_drawable() {
    use chrono::{Days, IsoWeek, Month, Months, Weekday};
    use hegel::generators as gs;
    printed_draw_lines(gs::default::<Weekday>());
    printed_draw_lines(gs::default::<Month>());
    printed_draw_lines(gs::default::<Days>());
    printed_draw_lines(gs::default::<Months>());
    printed_draw_lines(gs::default::<IsoWeek>());
}
