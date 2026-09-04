use crate::common::utils::printed_draw_lines;
use hegel::extras::jiff as jiff_gs;

#[test]
fn every_jiff_generator_prints_its_drawn_value() {
    printed_draw_lines(jiff_gs::dates());
    printed_draw_lines(jiff_gs::times());
    printed_draw_lines(jiff_gs::datetimes());
    printed_draw_lines(jiff_gs::timestamps());
    printed_draw_lines(jiff_gs::spans());
    printed_draw_lines(jiff_gs::signed_durations());
    printed_draw_lines(jiff_gs::offsets());
    printed_draw_lines(jiff_gs::zoneds());
}

fn render<T: hegel::PrettyPrintable>(value: &T) -> String {
    let mut doc = hegel::Document::new();
    value.pretty_print(doc.printer());
    doc.finish()
}

#[test]
fn jiff_values_print_as_constructor_expressions() {
    use jiff::civil::{date, datetime, time};

    assert_eq!(render(&date(2020, 2, 29)), "date(2020, 2, 29)");
    assert_eq!(render(&time(1, 2, 3, 40)), "time(1, 2, 3, 40)");
    assert_eq!(
        render(&datetime(2020, 1, 1, 0, 0, 0, 0)),
        "datetime(2020, 1, 1, 0, 0, 0, 0)"
    );
    assert_eq!(
        render(&jiff::Timestamp::new(12, 34).unwrap()),
        "Timestamp::new(12, 34).unwrap()"
    );
    assert_eq!(render(&jiff::Span::new()), "Span::new()");
    assert_eq!(
        render(&jiff::Span::new().years(1).seconds(30)),
        "Span::new().years(1).seconds(30)"
    );
    assert_eq!(
        render(&jiff::SignedDuration::new(-3, -500_000_000)),
        "SignedDuration::new(-3, -500000000)"
    );
    assert_eq!(
        render(&jiff::tz::Offset::from_seconds(3600).unwrap()),
        "Offset::from_seconds(3600).unwrap()"
    );
    let zoned = jiff::Timestamp::new(0, 0)
        .unwrap()
        .to_zoned(jiff::tz::TimeZone::UTC);
    assert_eq!(
        render(&zoned),
        "\"1970-01-01T00:00:00+00:00[UTC]\".parse::<Zoned>().unwrap()"
    );
}

#[test]
fn time_zones_print_as_constructor_expressions() {
    use jiff::tz::{Offset, TimeZone};
    assert_eq!(render(&TimeZone::UTC), "TimeZone::UTC");
    assert_eq!(render(&TimeZone::unknown()), "TimeZone::unknown()");
    assert_eq!(
        render(&TimeZone::fixed(Offset::constant(2))),
        "TimeZone::fixed(Offset::from_seconds(7200).unwrap())"
    );
    assert_eq!(
        render(&TimeZone::get("America/New_York").unwrap()),
        "TimeZone::get(\"America/New_York\").unwrap()"
    );
    let posix = TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap();
    assert!(render(&posix).contains("EST5EDT"));
}

#[test]
fn ambiguous_offsets_print_their_variant() {
    use jiff::tz::{AmbiguousOffset, Offset};
    let o = Offset::constant(1);
    assert_eq!(
        render(&AmbiguousOffset::Unambiguous { offset: o }),
        "AmbiguousOffset::Unambiguous { offset: Offset::from_seconds(3600).unwrap() }"
    );
    assert_eq!(
        render(&AmbiguousOffset::Gap {
            before: o,
            after: o
        }),
        "AmbiguousOffset::Gap {\n    before: Offset::from_seconds(3600).unwrap(),\n    after: Offset::from_seconds(3600).unwrap() }"
    );
    assert_eq!(
        render(&AmbiguousOffset::Fold {
            before: o,
            after: o
        }),
        "AmbiguousOffset::Fold {\n    before: Offset::from_seconds(3600).unwrap(),\n    after: Offset::from_seconds(3600).unwrap() }"
    );
}

#[test]
fn boxed_jiff_default_generators_are_drawable() {
    use hegel::generators as gs;
    use jiff::tz::{AmbiguousOffset, TimeZone};
    printed_draw_lines(gs::default::<TimeZone>());
    printed_draw_lines(gs::default::<AmbiguousOffset>());
}
