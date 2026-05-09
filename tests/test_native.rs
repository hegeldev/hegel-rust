#![cfg(feature = "native")]

mod common;

use common::utils::assert_all_examples;
use hegel::generators::{self as gs, Generator};

#[hegel::test]
fn native_integer_in_range(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<i32>().min_value(-100).max_value(100));
    assert!((-100..=100).contains(&n));
}

// `native_boolean_is_bool` (a `b || !b` tautology) and the open-range
// `native_u8_in_range` / `native_i64_in_range` smoke checks were
// deleted as part of D5 — the assertions were either logical
// tautologies or type-system tautologies (`u8` literally cannot be
// outside `u8::MIN..=u8::MAX`).  See `## 10. Test changelog` for the
// rationale.

/// Narrow-range u8: assert the generator respects user-supplied
/// `min_value` / `max_value` bounds.  This is a real behavioural claim
/// — a generator that ignored the bounds would (occasionally) produce
/// values outside `[10, 200]`.
#[hegel::test]
fn native_u8_respects_narrow_bounds(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<u8>().min_value(10).max_value(200));
    assert!(
        (10..=200).contains(&n),
        "u8 generator with min=10/max=200 yielded {n}",
    );
}

/// Narrow-range i64: same property, exercising the signed code path.
#[hegel::test]
fn native_i64_respects_narrow_bounds(tc: hegel::TestCase) {
    let n = tc.draw(gs::integers::<i64>().min_value(-1000).max_value(1000));
    assert!(
        (-1000..=1000).contains(&n),
        "i64 generator with min=-1000/max=1000 yielded {n}",
    );
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

// `native_multiple_draws` (asserting `a >= 0 && b >= 0` for non-negative
// generators — a tautology) is replaced with a cross-case
// independence-and-bounds check using `Hegel::new` directly so the
// post-run assertion runs after all test cases (no inter-test
// ordering dependency).
#[test]
fn native_multiple_draws_are_independent() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let differed = Arc::new(AtomicBool::new(false));
    let differed_clone = differed.clone();
    hegel::Hegel::new(move |tc: hegel::TestCase| {
        let a = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
        let b = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
        assert!(
            (0..=100).contains(&a) && (0..=100).contains(&b),
            "draws ({a}, {b}) outside [0,100]",
        );
        if a != b {
            differed_clone.store(true, Ordering::SeqCst);
        }
    })
    .run();
    // With i.i.d. draws over `[0, 100]` and 100 test cases (the default
    // `test_cases`), the probability of `a == b` in *every* case is
    // `(1/101)^100` — astronomically small.  A failure here points to a
    // generator-determinism or RNG-shareing regression.
    assert!(
        differed.load(Ordering::SeqCst),
        "expected at least one (a, b) case with a != b across the run",
    );
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

/// HirKind::Literal blocked by alphabet triggers Invalid for every
/// case — the body's after-draw code is unreachable.  Behavioural
/// claim: a counter incremented after the `tc.draw` call stays at zero
/// across the whole run.
#[test]
fn native_regex_literal_blocked_by_alphabet() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let post_draw = Arc::new(AtomicUsize::new(0));
    let counter = post_draw.clone();
    hegel::Hegel::new(move |tc: hegel::TestCase| {
        let _s = tc.draw(
            gs::from_regex("a")
                .fullmatch(true)
                .alphabet(gs::characters().categories(&["Nd"])),
        );
        // Reaching here would mean the digit-only alphabet somehow
        // accepted "a" — which contradicts the alphabet filter.
        counter.fetch_add(1, Ordering::SeqCst);
    })
    .settings(hegel::Settings::new().test_cases(10))
    .run();
    assert_eq!(
        post_draw.load(Ordering::SeqCst),
        0,
        "every case must be filtered Invalid before the body's after-draw code runs",
    );
}

/// HirKind::Class::Unicode empty after filtering: same shape as above.
/// `[a-z]+` filtered to digits has no overlap, so every case must be
/// Invalid.  The post-draw counter must stay at 0.
#[test]
fn native_regex_unicode_class_empty_after_filter() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let post_draw = Arc::new(AtomicUsize::new(0));
    let counter = post_draw.clone();
    hegel::Hegel::new(move |tc: hegel::TestCase| {
        let _s = tc.draw(
            gs::from_regex("[a-z]+")
                .fullmatch(true)
                .alphabet(gs::characters().categories(&["Nd"])),
        );
        counter.fetch_add(1, Ordering::SeqCst);
    })
    .settings(hegel::Settings::new().test_cases(10))
    .run();
    assert_eq!(
        post_draw.load(Ordering::SeqCst),
        0,
        "every case must be filtered Invalid before the body's after-draw code runs",
    );
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
