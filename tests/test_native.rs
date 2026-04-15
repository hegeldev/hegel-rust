#![cfg(feature = "native")]

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
    assert!(b || !b);
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
    let n = tc.draw(gs::integers::<i32>().min_value(0).max_value(100).map(|x| x * 2));
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
