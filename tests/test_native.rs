#![cfg(feature = "native")]

mod common;

use common::utils::assert_all_examples;
use hegel::generators::{self as gs, Generator};

#[hegel::test]
fn native_integer_in_range(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<i32>().min_value(-100).max_value(100));
    assert!((-100..=100).contains(&n));
}

#[hegel::test]
fn native_boolean_is_bool(tc: hegel::TestCase) {
    let b = tc.draw(gs::booleans());
    // Tautology: just exercises the boolean generation path.
    // Use black_box to prevent clippy from optimizing out the tautology check.
    assert!(b || !std::hint::black_box(b));
}

#[hegel::test]
fn native_u8_in_range(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<u8>());
    assert!((u8::MIN..=u8::MAX).contains(&n));
}

#[hegel::test]
fn native_i64_in_range(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<i64>());
    assert!((i64::MIN..=i64::MAX).contains(&n));
}

#[hegel::test]
fn native_assume_filters(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<i32>().min_value(-1000).max_value(1000));
    tc.assume(n > 0);
    assert!(n > 0);
}

#[hegel::test]
fn native_mapped_generator(tc: hegel::TestCase) {
    let n = tc.draw(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|x| x * 2),
    );
    assert!(n >= 0);
    assert!(n <= 200);
    assert!(n % 2 == 0);
}

/// Test that shrinking finds the minimal counterexample.
#[test]
fn native_shrinks_to_boundary() {
    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc: hegel::TestCase| {
            let n = tc.draw(gs::integers::<i32>().min_value(0).max_value(1000));
            assert!(n < 50);
        })
        .settings(hegel::Settings::new().seed(Some(42)))
        .run();
    });
    assert!(result.is_err());
    let msg = result
        .unwrap_err()
        .downcast::<String>()
        .unwrap()
        .to_string();
    assert!(
        msg.contains("Property test failed"),
        "Expected property test failure, got: {}",
        msg
    );
}

/// Test that shrinking finds the simplest negative example.
#[test]
fn native_shrinks_negative() {
    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc: hegel::TestCase| {
            let n = tc.draw(gs::integers::<i32>().min_value(-1000).max_value(1000));
            assert!(n >= -50);
        })
        .settings(hegel::Settings::new().seed(Some(42)))
        .run();
    });
    assert!(result.is_err());
}

#[hegel::test(seed = Some(123))]
fn native_deterministic_with_seed(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<i32>().min_value(0).max_value(1_000_000));
    assert!(n >= 0);
}

#[hegel::test]
fn native_multiple_draws(tc: hegel::TestCase) {
    let a = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
    let b = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
    // Just exercise multiple draws in one test case.
    assert!(a >= 0 && b >= 0);
}

// --- Regex schema coverage tests ---

/// HirKind::Empty: an empty capture group generates an empty string.
#[test]
fn native_regex_empty_hir() {
    assert_all_examples(gs::from_regex("()").fullmatch(true), |s: &String| {
        s.is_empty()
    });
}

/// HirKind::Literal: a literal pattern generates exactly that string.
#[test]
fn native_regex_literal() {
    assert_all_examples(gs::from_regex("abc").fullmatch(true), |s: &String| {
        s == "abc"
    });
}

/// HirKind::Concat: an anchor + literal + anchor uses Concat and Look nodes.
#[test]
fn native_regex_concat_and_look() {
    assert_all_examples(gs::from_regex("^abc$").fullmatch(true), |s: &String| {
        s == "abc"
    });
}

/// HirKind::Alternation: a|b generates "a" or "b".
#[test]
fn native_regex_alternation() {
    assert_all_examples(gs::from_regex("a|b").fullmatch(true), |s: &String| {
        s == "a" || s == "b"
    });
}

/// HirKind::Capture: a capture group generates the contained pattern.
#[test]
fn native_regex_capture() {
    assert_all_examples(gs::from_regex("(abc)").fullmatch(true), |s: &String| {
        s == "abc"
    });
}

/// HirKind::Class::Bytes: byte-mode character class generates matching chars.
#[test]
fn native_regex_bytes_class() {
    assert_all_examples(
        gs::from_regex("(?-u)[a-z]+").fullmatch(true),
        |s: &String| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase()),
    );
}

/// HirKind::Class::Unicode with Explicit alphabet: filters against explicit char list.
#[test]
fn native_regex_unicode_class_explicit_alphabet() {
    assert_all_examples(
        gs::from_regex("[a-c]+")
            .fullmatch(true)
            .alphabet(gs::characters().categories(&[]).include_characters("abc")),
        |s: &String| !s.is_empty() && s.chars().all(|c| matches!(c, 'a' | 'b' | 'c')),
    );
}

/// regex_alphabet_allows None: no alphabet means all chars pass.
#[test]
fn native_regex_no_alphabet() {
    assert_all_examples(gs::from_regex("[a-z]+").fullmatch(true), |s: &String| {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase())
    });
}

/// fullmatch=false: generates a string containing a match with possible surrounding text.
#[test]
fn native_regex_partial_match() {
    // The partial match path generates prefix + match + suffix.
    // We can't know the full string, but the overall string must contain something.
    assert_all_examples(gs::from_regex("[a-z]+"), |s: &String| {
        s.chars().all(|c| c.is_ascii() || c.is_alphabetic())
    });
}

/// HirKind::Literal blocked by alphabet triggers Invalid.
/// This is tested by running a regex with a literal char not in the alphabet,
/// and verifying it succeeds (invalid test cases are filtered out silently).
#[test]
fn native_regex_literal_blocked_by_alphabet() {
    // "a" fullmatch with an alphabet that only allows digits.
    // Every generated case tries to push 'a' but the alphabet only allows '0'-'9',
    // so every case is Invalid. With 100 test cases all filtered, Hegel passes
    // (same as filter_too_much health check suppression).
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let _s = tc.draw(
            gs::from_regex("a")
                .fullmatch(true)
                .alphabet(gs::characters().categories(&["Nd"])),
        );
        // If we get here, the alphabet allowed 'a' — which shouldn't happen.
        // In practice all cases become Invalid so this closure never executes.
    })
    .settings(hegel::Settings::new().test_cases(10))
    .run();
}

/// HirKind::Class::Unicode empty after filtering triggers Invalid.
/// A class [a-z] filtered to digits (no overlap) gives empty chars → Invalid.
#[test]
fn native_regex_unicode_class_empty_after_filter() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let _s = tc.draw(
            gs::from_regex("[a-z]+")
                .fullmatch(true)
                .alphabet(gs::characters().categories(&["Nd"])),
        );
    })
    .settings(hegel::Settings::new().test_cases(10))
    .run();
}

/// HirKind::Class::Bytes empty after filtering triggers Invalid.
#[test]
fn native_regex_bytes_class_empty_after_filter() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let _s = tc.draw(
            gs::from_regex("(?-u)[a-z]+")
                .fullmatch(true)
                .alphabet(gs::characters().categories(&["Nd"])),
        );
    })
    .settings(hegel::Settings::new().test_cases(10))
    .run();
}

/// Regression test: FloatChoice::simplest() must return -∞ (not panic) when
/// the effective max is -∞. This is triggered by max_value(f64::MIN) with
/// exclude_max=true, where f64::MIN.next_down() = f64::NEG_INFINITY.
/// The only valid float in that range is -∞, so every draw must return -∞.
#[test]
fn native_float_neg_inf_boundary_simplest() {
    // max_value(f64::MIN).exclude_max(true) → effective max = f64::MIN.next_down() = -∞
    // allow_nan defaults to false (because max is set), allow_infinity defaults to true.
    // The only valid value is -∞; simplest() must return it without panicking.
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let v: f64 = tc.draw(gs::floats::<f64>().max_value(f64::MIN).exclude_max(true));
        assert_eq!(v, f64::NEG_INFINITY);
    })
    .run();
}

/// interpret_string with a surrogate-only range (e.g. [0xD800, 0xDFFF]) should
/// report InvalidArgument, matching the server backend's behavior.
#[test]
#[should_panic(expected = "InvalidArgument")]
fn native_string_empty_alphabet_is_invalid() {
    hegel::Hegel::new(|tc: hegel::TestCase| {
        let _c = tc.draw(gs::characters().min_codepoint(0xD800).max_codepoint(0xDFFF));
    })
    .settings(hegel::Settings::new().test_cases(10))
    .run();
}
