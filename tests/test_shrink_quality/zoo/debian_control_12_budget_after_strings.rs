//! From hegel-zoo `rust/debian-control`, bug debian-control/12, test `bad_operators_do_not_panic`.
//!
//! Every text `"{name}{ws}({ws}{op}{ws}{ver})"` with an operator outside dpkg's set is accepted
//! by the strict parser and then `version()` panics, so the whole input space fails and the
//! shortlex ideal is the all-minimal sequence: `"libc6(=>0)"` — name from the first branch and
//! first table entry, first operator, version without epoch or revision (the `int(0, 4)` epoch
//! gate at 1 is one draw shorter than an epoch), integer 0, no extra characters, no whitespace.
//!
//! The replica ports the zoo's `arb_name`, `arb_version` and `ws0` draw for draw. Every run
//! starts from the all-minimal `"libc6(=>0:0)"` (epoch present, 11 draws); the zoo and the
//! replica both see runs end on the 10-draw form with the version integer left at 44, 64, 191,
//! 564, … although every value fails: the move from the epoch form to the shorter one leaves
//! the integer holding a value it then never lowers.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

const NAMES: &[&str] = &[
    "libc6",
    "python3",
    "debhelper",
    "debhelper-compat",
    "dh-python",
    "g++",
    "libstdc++6",
    "perl",
    "cdbs",
    "python3-dulwich",
    "libgtk-3-0",
    "gcc-13-base",
    "zlib1g",
    "libssl3t64",
    "rustc",
    "cargo",
    "dpkg-dev",
    "pkg-config",
    "samba",
    "libx11-6",
    "a",
    "b",
    "c",
    "0ad",
    "x.y",
    "foo+bar",
];
const BAD_OPS: &[&str] = &["=>", "<>", ">>>", "==", "=<", "><", "<<<", "<=>", "=="];
const WS0: &[&str] = &["", " ", " ", " ", "  ", "\t", "\n ", "\n\t"];

fn pick<T: Clone + Send + Sync + 'static>(tc: &TestCase, items: &[T], n: &mut u32) -> T {
    *n += 1;
    tc.draw_silent(gs::sampled_from(items.to_vec()))
}

fn int(tc: &TestCase, lo: u32, hi: u32, n: &mut u32) -> u32 {
    *n += 1;
    tc.draw_silent(gs::integers::<u32>().min_value(lo).max_value(hi))
}

fn chars_from(tc: &TestCase, alphabet: &str, count: u32, n: &mut u32) -> String {
    let chars: Vec<char> = alphabet.chars().collect();
    (0..count).map(|_| pick(tc, &chars, n)).collect()
}

fn arb_name(tc: &TestCase, n: &mut u32) -> String {
    match int(tc, 0, 5, n) {
        0..=2 => pick(tc, NAMES, n).to_string(),
        3 => {
            let first = chars_from(tc, "abcdefghijklmnopqrstuvwxyz0123456789", 1, n);
            let len = int(tc, 1, 8, n);
            first + &chars_from(tc, "abcdefghijklmnopqrstuvwxyz0123456789+.-", len, n)
        }
        4 => chars_from(tc, "abcdefghijklmnopqrstuvwxyz0123456789", 1, n),
        _ => {
            let first = chars_from(tc, "ABCDEFabcdef0123456789", 1, n);
            let len = int(tc, 0, 6, n);
            first + &chars_from(tc, "abcdefXYZ0123456789+.-", len, n)
        }
    }
}

fn arb_version(tc: &TestCase, n: &mut u32) -> String {
    let mut s = String::new();
    if int(tc, 0, 4, n) == 0 {
        s.push_str(&int(tc, 0, 99, n).to_string());
        s.push(':');
    }
    let has_revision = int(tc, 0, 2, n) != 0;
    s.push_str(&int(tc, 0, 999, n).to_string());
    let upstream_alpha = if has_revision {
        "0123456789abcdefghijklmnopqrstuvwxyz.+~-"
    } else {
        "0123456789abcdefghijklmnopqrstuvwxyz.+~"
    };
    let len = int(tc, 0, 8, n);
    s.push_str(&chars_from(tc, upstream_alpha, len, n));
    if has_revision {
        s.push('-');
        s.push_str(&int(tc, 0, 99, n).to_string());
        let len = int(tc, 0, 4, n);
        s.push_str(&chars_from(
            tc,
            "0123456789abcdefghijklmnopqrstuvwxyz.+~",
            len,
            n,
        ));
    }
    s
}

#[derive(Debug)]
#[allow(dead_code)]
struct Case {
    text: String,
    draws: u32,
}

fn draw(tc: &TestCase) -> Case {
    let mut n = 0;
    let name = arb_name(tc, &mut n);
    let op = pick(tc, BAD_OPS, &mut n);
    let ver = arb_version(tc, &mut n);
    let (w1, w2, w3) = (
        pick(tc, WS0, &mut n),
        pick(tc, WS0, &mut n),
        pick(tc, WS0, &mut n),
    );
    Case {
        text: format!("{name}{w1}({w2}{op}{w3}{ver})"),
        draws: n,
    }
}

#[test]
fn bounded_integer_is_fully_shrunk_after_the_strings() {
    let ideal = Case {
        text: "libc6(=>0)".to_string(),
        draws: 10,
    };
    assert_shrinks_to(&ideal, 20, 100, draw, |_| true);
}
