//! From hegel-zoo `rust/sprintf`, bug sprintf/13, test `negative_star_width_means_left_adjustment`.
//!
//! A negative `*` width argument means "left-adjust to the absolute width" in C; the crate
//! ignores it, so the output lacks the right padding whenever `|width| > len(formatted value)`.
//! The test's draws, in order: conversion (`d`/`f`/`s`), five `chance(p)` flag coins
//! (`n(0, 99) < p`), a width kind (`n(0, 3)`, then `n(0, 40)` for kinds 1–3), a precision coin
//! (`chance(70)`, then `n(0, 6)`), the star width magnitude `n(1, 30)`, and the argument
//! (`n(-1000, 1000)` for `d`). The test forces `zero`/`left`/`alt` off, so only `space`/`plus`
//! matter, and they only make the value one character longer.
//!
//! Shortlex ideal, as draws: `[0, 0, 0, 0, 0, 0, 0, 70, 3, 0]` — `%d`, all five flag draws 0
//! (which *sets* every flag), no width, precision coin 70 (the smallest value that skips the
//! precision draw), width 3 (`"+0"` is two characters), value 0. The 11-draw shape with the
//! precision taken is a local minimum at `[…, 0, 0, 2, 0]`; leaving it needs the coin raised and
//! the precision deleted, and a shrinker that then lands at an arbitrary point of the shorter
//! shape must still zero the value. A human would write `vsprintf("%*d", [-2], 0)`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw(tc: &TestCase) -> Vec<i64> {
    let mut d = Vec::new();
    let mut n = |lo, hi| {
        let v = tc.draw_silent(gs::integers::<i64>().min_value(lo).max_value(hi));
        d.push(v);
        v
    };
    let conv = n(0, 2);
    for _ in 0..5 {
        n(0, 99);
    }
    if n(0, 3) != 0 {
        n(0, 40);
    }
    if n(0, 99) < 70 {
        n(0, 6);
    }
    n(1, 30);
    match conv {
        0 => {
            n(-1000, 1000);
        }
        1 => {
            n(1, 999_999);
        }
        _ => {
            let len = n(0, 12);
            for _ in 0..len {
                n(0x20, 0x7e);
            }
        }
    }
    d
}

fn output_lacks_padding(d: &[i64]) -> bool {
    let mut i = 0;
    let mut next = || {
        let v = d[i];
        i += 1;
        v
    };
    let conv = next();
    let _alt = next() < 20;
    let _zero = next() < 30;
    let _left = next() < 25;
    let space = next() < 20;
    let plus = next() < 20;
    if next() != 0 {
        next();
    }
    let precision = if next() < 70 { Some(next()) } else { None };
    let width = next();
    let formatted = match conv {
        0 => {
            let v = next();
            let mut digits = v.unsigned_abs().to_string();
            if let Some(p) = precision {
                if p == 0 && v == 0 {
                    digits.clear();
                }
                while (digits.len() as i64) < p {
                    digits.insert(0, '0');
                }
            }
            let sign = if v < 0 {
                "-"
            } else if plus {
                "+"
            } else if space {
                " "
            } else {
                ""
            };
            format!("{sign}{digits}")
        }
        1 => {
            let v = next() as f64 / 1000.0;
            let body = format!("{:.*}", precision.unwrap_or(6) as usize, v);
            let sign = if plus {
                "+"
            } else if space {
                " "
            } else {
                ""
            };
            format!("{sign}{body}")
        }
        _ => {
            let len = next();
            let s: String = (0..len).map(|_| next() as u8 as char).collect();
            match precision {
                Some(p) => s.chars().take(p as usize).collect(),
                None => s,
            }
        }
    };
    width > formatted.len() as i64
}

fn ideal() -> Vec<i64> {
    vec![0, 0, 0, 0, 0, 0, 0, 70, 3, 0]
}

#[test]
fn the_ideal_does_fail() {
    assert!(output_lacks_padding(&ideal()));
    assert!(!output_lacks_padding(&[0, 0, 0, 0, 0, 0, 0, 70, 2, 0]));
    assert!(output_lacks_padding(&[0, 0, 0, 0, 0, 0, 0, 70, 5, -412]));
    assert!(!output_lacks_padding(&[0, 0, 0, 0, 0, 0, 0, 70, 4, -412]));
}

#[test]
fn irrelevant_argument_is_zeroed() {
    assert_shrinks_to(&ideal(), 30, 100, draw, |d| output_lacks_padding(d));
}
