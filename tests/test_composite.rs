// Compile-time behaviour of #[hegel::composite] (successful expansion and
// the various malformed-input error messages) lives in
// tests/compile/{pass,fail}/composite_*.rs, driven by `trybuild`.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::composite]
fn composite_integer_generator(tc: TestCase, lower: i32, upper: i32, offset: i32) -> i32 {
    let x = tc.draw(gs::integers::<i32>().min_value(lower).max_value(upper));
    x + offset
}

#[hegel::test]
fn test_passing_composite_generation(tc: TestCase) {
    let x = tc.draw(composite_integer_generator(0, 100, 1));
    assert!(x > 0);
}
