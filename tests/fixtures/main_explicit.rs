//! Fixture binary: `#[hegel::main]` combined with an explicit test case; the
//! explicit value must actually be fed to the body.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 1)]
#[hegel::explicit_test_case(x = 77i32)]
fn main(tc: TestCase) {
    let x: i32 = tc.draw(gs::integers());
    if x == 77 {
        panic!("got explicit value");
    }
}
