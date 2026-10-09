//! Fixture binary: a `#[hegel::main]` whose property always fails through
//! `tc.fail`, from a helper that raises the failure under one of two
//! labels, so the label is the failure's origin and heads its report.

use hegel::TestCase;
use hegel::generators as gs;

fn check(tc: &TestCase, x: i32) {
    tc.note(&format!("x = {x}"));
    if x % 2 == 0 {
        tc.fail("even");
    }
    tc.fail("odd");
}

#[hegel::main]
fn main(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(50));
    check(&tc, x);
}
