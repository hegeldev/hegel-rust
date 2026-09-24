//! From hegel-zoo `rust/humansize`, bug humansize/2, test `grouped_negative_renderings_keep_their_decimals`.
//!
//! humansize's thousands-separator path prints the fraction as `round(fpart · 10^places) as u64`
//! with no carry into the integer part; for a negative value `fpart` is negative, so the cast
//! saturates to 0 and every decimal prints as `0`, and with `places = 0` the mantissa is
//! truncated where the plain path (`{:.0}`) rounds. The test compares the grouped rendering with
//! the plain one regrouped. The formatter is modelled below operation for operation; the plain
//! path is Rust's `{:.*}`.
//!
//! Draws: the grouping options (preset `n(0, 3)` [+ 2 booleans], `bits`, `places n(0, 10)`,
//! `zeroes n(0, 5)`, `fixed` coin [+ `n(0, 8)`], `long`, `space`, `suffix n(0, 4)`, `sep` coin
//! [+ `n(0, 4)`], and `n(0, 4)` again if the coin gave `None`), then the magnitude (an 8-way
//! branch). Shortlex ideal: default options (decimal, `places 0`, `fixed None`) and `mag = 1500`
//! → `-1.5 kB`, plain `"-2kB"`, grouped `"-1kB"`: `1500` is the smallest magnitude with a
//! fraction ≥ .5 in some unit, and `fixed: None` saves a draw.
//!
//! The failing magnitudes are `{m : frac(m / 1000^k) ≥ .5}`, a sparse periodic set, so bisecting
//! from `1_500_000` towards 0 passes through `750_000` and `1_125_000` (both render the same
//! either way) and never finds `1500`; and `places 1, mag 1050` is a smaller magnitude bought
//! with a larger, earlier draw. A human writes
//! `format_size_i(-1500, DECIMAL.thousands_separator(','))`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn n(tc: &TestCase, lo: i64, hi: i64) -> i64 {
    tc.draw_silent(gs::integers::<i64>().min_value(lo).max_value(hi))
}

fn nu(tc: &TestCase, lo: u64, hi: u64) -> u64 {
    tc.draw_silent(gs::integers::<u64>().min_value(lo).max_value(hi))
}

fn boolean(tc: &TestCase) -> bool {
    tc.draw_silent(gs::booleans())
}

fn sample<T: Clone + Send + Sync + 'static>(tc: &TestCase, xs: Vec<T>) -> T {
    tc.draw_silent(gs::sampled_from(xs))
}

#[derive(Debug)]
struct Opts {
    kilo_binary: bool,
    places: usize,
    zeroes: usize,
    fixed: Option<usize>,
}

#[derive(Debug)]
struct Case {
    opts: Opts,
    mag: u64,
}

fn arb_grouping_opts(tc: &TestCase) -> Opts {
    let preset = n(tc, 0, 3);
    let kilo_binary = match preset {
        0 => false,
        1 | 2 => true,
        _ => {
            let kb = boolean(tc);
            let _units_binary = boolean(tc);
            kb
        }
    };
    let _bits = boolean(tc);
    let places = n(tc, 0, 10) as usize;
    let zeroes = n(tc, 0, 5) as usize;
    let fixed = if boolean(tc) {
        None
    } else {
        Some(n(tc, 0, 8) as usize)
    };
    let _long = boolean(tc);
    let _space = boolean(tc);
    let _suffix = n(tc, 0, 4);
    let sep = if boolean(tc) { None } else { Some(n(tc, 0, 4)) };
    if sep.is_none() {
        n(tc, 0, 4);
    }
    Opts {
        kilo_binary,
        places,
        zeroes,
        fixed,
    }
}

fn arb_mag(tc: &TestCase) -> u64 {
    match n(tc, 0, 7) {
        0 => nu(tc, 0, u64::MAX),
        1 => nu(tc, 0, 2100),
        2 => nu(tc, 0, 1 << 53),
        3 | 4 => {
            let d: u64 = if boolean(tc) { 1000 } else { 1024 };
            let e = n(tc, 1, 6) as u32;
            let delta = n(tc, -3, 3);
            let mult = sample(
                tc,
                vec![1u64, 1, 1, 2, 7, 999, 1023, 1024, 125, 128, 1999, 2048],
            );
            d.pow(e).saturating_mul(mult).saturating_add_signed(delta)
        }
        5 => {
            let d: u64 = if boolean(tc) { 1000 } else { 1024 };
            let e = n(tc, 1, 5) as u32;
            let m = sample(
                tc,
                vec![
                    1005u64, 1015, 1995, 1999, 9995, 9999, 10005, 19995, 1000005, 999995,
                ],
            );
            let scale = sample(tc, vec![1000u64, 10_000, 1_000_000]);
            ((d.pow(e) as u128 * m as u128) / scale as u128).min(u64::MAX as u128) as u64
        }
        6 => {
            let d: u64 = if boolean(tc) { 1000 } else { 1024 };
            let e = n(tc, 1, 5) as u32;
            let permille = nu(tc, 900, 2100);
            ((d.pow(e) as u128 * permille as u128) / 1000).min(u64::MAX as u128) as u64
        }
        _ => sample(
            tc,
            vec![
                0,
                1,
                2,
                999,
                1000,
                1001,
                1023,
                1024,
                1025,
                1500,
                1999,
                2000,
                u64::MAX,
                u64::MAX - 1,
                1 << 63,
                (1 << 53) - 1,
                1 << 53,
                (1 << 53) + 1,
            ],
        ),
    }
}

fn draw(tc: &TestCase) -> Case {
    let opts = arb_grouping_opts(tc);
    let mag = arb_mag(tc).min((1 << 53) - 1);
    Case { opts, mag }
}

fn f64_eq(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= f64::EPSILON
}

/// humansize's `ISizeFormatter::fmt`, number part only: `(plain, grouped)`.
fn renderings(c: &Case) -> (String, String) {
    let value = -(c.mag as i64);
    let divider = if c.opts.kilo_binary { 1024.0 } else { 1000.0 };
    let mut size = value as f64;
    let mut idx = 0usize;
    if let Some(fixed) = c.opts.fixed {
        while idx != fixed {
            size /= divider;
            idx += 1;
        }
    } else {
        while size.abs() >= divider {
            size /= divider;
            idx += 1;
            if idx == 8 {
                break;
            }
        }
    }
    let ipart = size.trunc();
    let fpart = size - ipart;
    let places = if f64_eq(fpart, 0.0) {
        c.opts.zeroes
    } else {
        c.opts.places
    };

    let mut out = Vec::new();
    let mut fraction = (fpart * 10f64.powi(places as i32)).round() as u64;
    for _ in 0..places {
        out.push(b'0' + (fraction % 10) as u8);
        fraction /= 10;
    }
    if places > 0 {
        out.push(b'.');
    }
    let mut integer_part = ipart.abs();
    let mut digit_count = 0;
    loop {
        if digit_count == 3 {
            out.push(b',');
            digit_count = 0;
        }
        out.push(b'0' + (integer_part % 10.0) as u8);
        integer_part /= 10.0;
        digit_count += 1;
        if integer_part < 1.0 {
            break;
        }
    }
    if size.is_sign_negative() {
        out.push(b'-');
    }
    out.reverse();
    let grouped = String::from_utf8(out).unwrap();

    let plain = format!("{:.*}", places, size);
    let (sign, digits) = match plain.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", plain.as_str()),
    };
    let (int_part, frac) = match digits.find('.') {
        Some(i) => (&digits[..i], &digits[i..]),
        None => (digits, ""),
    };
    let mut g = String::new();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            g.push(',');
        }
        g.push(ch);
    }
    (format!("{sign}{g}{frac}"), grouped)
}

fn renderings_differ(c: &Case) -> bool {
    let (plain, grouped) = renderings(c);
    plain != grouped
}

fn default_opts() -> Opts {
    Opts {
        kilo_binary: false,
        places: 0,
        zeroes: 0,
        fixed: None,
    }
}

fn ideal() -> Case {
    Case {
        opts: default_opts(),
        mag: 1500,
    }
}

#[test]
fn the_ideal_does_fail() {
    assert_eq!(renderings(&ideal()), ("-2".to_string(), "-1".to_string()));
    for m in 0..1500 {
        let c = Case {
            opts: default_opts(),
            mag: m,
        };
        assert!(!renderings_differ(&c), "{m} fails before 1500");
    }
    let fixed_kb = Case {
        opts: Opts {
            fixed: Some(1),
            ..default_opts()
        },
        mag: 501,
    };
    assert_eq!(renderings(&fixed_kb), ("-1".into(), "-0".into()));
    let two_places = Case {
        opts: Opts {
            places: 2,
            ..default_opts()
        },
        mag: 1006,
    };
    assert_eq!(renderings(&two_places), ("-1.01".into(), "-1.00".into()));
    for (mag, differs) in [(1_500_000, true), (750_000, false), (1_125_000, false)] {
        let c = Case {
            opts: default_opts(),
            mag,
        };
        assert_eq!(renderings_differ(&c), differs, "{mag}");
    }
}

#[test]
#[ignore = "shrinker: integer bisection passes straight through a sparse failing set"]
fn negative_grouping_bug_shrinks_to_minus_1500() {
    assert_shrinks_to(&ideal(), 30, 100, draw, renderings_differ);
}
