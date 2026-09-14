//! Fixture binary: a failing `#[hegel::main]` with compiled-in
//! `print_blob = false`. Whether the reproducer line reaches stderr shows
//! from outside whether the compiled-in settings applied on top of the
//! default profile.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(print_blob = false)]
fn main(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(50));
    assert!(x < 0, "got nonneg {}", x);
}
