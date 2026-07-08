use crate::common::utils::printed_draw_lines;
use hegel::extras::rand as rand_gs;

#[test]
fn drawn_rngs_print_a_placeholder() {
    let lines = printed_draw_lines(rand_gs::randoms());
    assert!(lines[0].contains("ArtificialRandom"), "{lines:?}");
}
