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
