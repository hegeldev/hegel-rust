//! From hegel-zoo `rust/bytesize`, bug bytesize/1, test `decimal_texts_denoting_whole_bytes_parse_exactly`.
//!
//! The test builds a text `"<int_part>.<frac> <unit>"` denoting a whole number of bytes and
//! checks it parses exactly. bytesize computes `mantissa_f64 * factor` and truncates, which is
//! one short whenever the f64 nearest to the mantissa lies below it: a sparse, irregular set of
//! (int_part, frac) pairs — for one fraction digit and factor 1000 it is int_part 32 with digit
//! 3, 64–65 with digit 1, 128–130 with digits 2 or 7, 1024… with digit 1, and so on.
//!
//! Draws, in the zoo test's order: unit, spelling, digit count, a three-way branch choosing the
//! integer's range, the integer, the fraction digits. Shortlex ideal (factor 1000, one digit,
//! the smallest int_part with any digit): `32.3`. Moving from any other local minimum (`64.1`,
//! `1024.1`, `1.001`, …) to it needs the integer lowered *and* the digit changed in the same
//! step; lowering the integer alone leaves the sparse set.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug)]
struct Text {
    factor: u64,
    int_part: u64,
    frac: String,
}

fn zoo_text(tc: &TestCase) -> Text {
    let factors: Vec<u64> = vec![
        1_000,
        1_000_000,
        1_000_000_000,
        1_000_000_000_000,
        1_000_000_000_000_000,
        1_000_000_000_000_000_000,
    ];
    let factor = tc.draw_silent(gs::sampled_from(factors));
    let _spelling_short = tc.draw_silent(gs::booleans());
    let k = tc.draw_silent(gs::integers::<usize>().min_value(1).max_value(3));
    let int_part: u64 = match tc.draw_silent(gs::integers::<u8>().min_value(0).max_value(2)) {
        0 => tc.draw_silent(gs::integers::<u64>().min_value(0).max_value(9_999)),
        1 => tc.draw_silent(gs::integers::<u64>().min_value(0).max_value(99)),
        _ => tc.draw_silent(
            gs::integers::<u64>()
                .min_value(0)
                .max_value(((1u64 << 53) / factor).max(1) - 1),
        ),
    };
    let frac: String = (0..k)
        .map(|_| char::from(b'0' + tc.draw_silent(gs::integers::<u8>().min_value(0).max_value(9))))
        .collect();
    Text {
        factor,
        int_part,
        frac,
    }
}

fn zoo_text_mis_parses(t: &Text) -> bool {
    let n: u128 = format!("{}{}", t.int_part, t.frac).parse().unwrap();
    let scale = 10u128.pow(t.frac.len() as u32);
    let exact = n * t.factor as u128 / scale;
    if exact >= 1 << 53 {
        return false;
    }
    let mantissa: f64 = format!("{}.{}", t.int_part, t.frac).parse().unwrap();
    let parsed = (mantissa * t.factor as f64) as u128;
    parsed != exact
}

#[test]
#[ignore = "shrinker: integer bisection passes straight through a sparse failing set"]
fn simultaneous_integer_and_digit_change_is_needed() {
    let ideal = Text {
        factor: 1_000,
        int_part: 32,
        frac: "3".to_string(),
    };
    assert_shrinks_to(&ideal, 20, 100, zoo_text, zoo_text_mis_parses);
}
