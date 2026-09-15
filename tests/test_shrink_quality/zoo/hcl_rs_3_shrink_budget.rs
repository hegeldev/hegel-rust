//! From hegel-zoo `rust/hcl-rs`, bug hcl-rs/3, test `props::prop_parse_negated_integer_literal_is_nonpositive`.
//!
//! `-n` for any `n ≥ 2^63` misparses. The generator is
//! `one_of!(integers::<u64>(), sampled_from([2^63, 2^63 + 1, u64::MAX]))`, so the shortlex
//! ideal is branch 0 with `n = 2^63`.
//!
//! The zoo's runs ended at `u64::MAX − 499` and `u64::MAX − 500`: once the deterministic passes
//! leave `n` at `u64::MAX` (everything below `2^63` passes, so the bisection towards 0 fails),
//! a pass stepping it down one unit at a time and re-run while it improves spends the whole
//! `MAX_SHRINKS = 500` budget. A human would write `n = 2^63`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug)]
#[allow(dead_code)]
struct Case {
    branch: u8,
    n: u64,
}

fn draw(tc: &TestCase) -> Case {
    let branch = tc.draw_silent(gs::integers::<u8>().min_value(0).max_value(1));
    let n = if branch == 0 {
        tc.draw_silent(gs::integers::<u64>())
    } else {
        tc.draw_silent(gs::sampled_from(vec![
            1u64 << 63,
            (1u64 << 63) + 1,
            u64::MAX,
        ]))
    };
    Case { branch, n }
}

fn negation_misparses(c: &Case) -> bool {
    c.n >= 1u64 << 63
}

#[test]
fn u64_above_2_pow_63_shrinks_to_2_pow_63() {
    let ideal = Case {
        branch: 0,
        n: 1u64 << 63,
    };
    assert_shrinks_to(&ideal, 20, 100, draw, negation_misparses);
}
