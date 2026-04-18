//! `#[hegel::composite]` on a well-formed function expands into code that
//! compiles.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::composite]
fn composite_integer_generator(tc: TestCase, n: i32) -> i32 {
    tc.draw(gs::integers::<i32>()) + n
}

fn main() {}
