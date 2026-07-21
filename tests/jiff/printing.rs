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
