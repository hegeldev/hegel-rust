//! Fixture binary: a `#[hegel::main]` whose property always fails, so the
//! process must exit nonzero with the assertion message on stderr.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 100)]
fn main(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(50));
    assert!(x < 0, "got nonneg {}", x);
}
