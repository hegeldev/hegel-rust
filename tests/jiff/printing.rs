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

#[test]
fn boxed_jiff_default_generators_are_drawable() {
    use hegel::generators as gs;
    use jiff::tz::{AmbiguousOffset, TimeZone};
    printed_draw_lines(gs::default::<TimeZone>());
    printed_draw_lines(gs::default::<AmbiguousOffset>());
}
