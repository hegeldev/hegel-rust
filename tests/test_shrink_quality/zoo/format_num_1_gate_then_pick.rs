//! From hegel-zoo `rust/format_num`, bug format_num/1, test `non_finite_values_format_as_text`.
//!
//! `format_num` formats `inf` with an integer type (`b`, `o`, `x`, `X`) as the binary digits of
//! `i64::MAX` instead of the text `inf`. The test draws a type (`pick` of nine, `b` is index 4),
//! then a format spec as a chain of `chance(p) = n(0, 99) < p` coins, several of which gate a
//! further `pick`/`n` draw — zero flag, alignment (+ pick, + fill), sign (+ pick), alternate
//! form, width, grouping, precision — then the non-finite value (`pick` of four). Every spec
//! fails for an integer type.
//!
//! Shortlex ideal, as draws: `[4, 0, 50, 0, 0, 0]` — type `b`; `zero` coin 0 (which also skips
//! the alignment draws); sign coin 50, the smallest value that skips the sign pick; `alt` coin 0;
//! width 0; value `inf`: `format("#00b", inf)`. The all-zero spec `[4, 0, 0, 0, 0, 0, 0]` is one
//! draw longer (sign coin 0 *and* a sign pick) and is where a seed whose first failure has the
//! sign gate closed starts; from there the sign coin must be raised while the pick is deleted.
//! A human would write `format("b", inf)`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

const TYPES: [char; 9] = ['f', 'e', '%', 'd', 'b', 'o', 'x', 'X', 's'];
const INT_TYPES: [char; 5] = ['b', 'o', 'd', 'x', 'X'];

struct Rec<'a> {
    tc: &'a TestCase,
    draws: Vec<i64>,
}

impl Rec<'_> {
    fn n(&mut self, lo: i64, hi: i64) -> i64 {
        let v = self
            .tc
            .draw_silent(gs::integers::<i64>().min_value(lo).max_value(hi));
        self.draws.push(v);
        v
    }

    fn chance(&mut self, pct: i64) -> bool {
        self.n(0, 99) < pct
    }
}

fn draw(tc: &TestCase) -> Vec<i64> {
    let mut r = Rec {
        tc,
        draws: Vec::new(),
    };
    let typ = TYPES[r.n(0, 8) as usize];
    let integer = INT_TYPES.contains(&typ);
    let zero = r.chance(25);
    let align = if zero || r.chance(30) {
        false
    } else {
        r.n(0, 3);
        true
    };
    if align && r.chance(60) {
        r.n(0x20, 0x7e);
    }
    if r.chance(50) {
        r.n(0, 2);
    }
    if typ != '%' {
        r.chance(30);
    }
    if zero || r.chance(50) {
        r.n(0, 30);
    }
    if !integer {
        if !r.chance(40) && typ == 'd' {
            r.chance(40);
        }
    } else if typ == 'd' {
        r.chance(40);
    }
    if !integer && r.chance(80) {
        r.n(0, 20);
    }
    r.n(0, 3);
    r.draws
}

fn formats_inf_as_digits(d: &[i64]) -> bool {
    matches!(d[0], 4..=7)
}

#[test]
fn the_ideal_does_fail() {
    assert!(formats_inf_as_digits(&[4, 0, 50, 0, 0, 0]));
    assert!(!formats_inf_as_digits(&[3, 0, 50, 0, 0, 0]));
}

#[test]
fn sign_gate_is_raised_and_pick_deleted() {
    assert_shrinks_to(&vec![4, 0, 50, 0, 0, 0], 30, 100, draw, |d| {
        formats_inf_as_digits(d)
    });
}
