use crate::common::utils::printed_draw_lines;
use hegel::extras::serde_json as json_gs;

#[test]
fn every_serde_json_generator_prints_its_drawn_value() {
    printed_draw_lines(json_gs::numbers());
    printed_draw_lines(json_gs::values());
}

#[cfg(feature = "serde_json_raw_value")]
#[test]
fn raw_values_print_their_drawn_value() {
    printed_draw_lines(json_gs::raw_values());
}
