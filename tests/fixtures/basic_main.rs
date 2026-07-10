//! Fixture binary for `tests/test_hegel_main.rs`: a `#[hegel::main]` entry
//! point with an attribute-set test-case count, so the driver tests can
//! exercise default runs, CLI overrides, and unknown-argument handling
//! against a real prebuilt binary.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 7)]
fn main(tc: TestCase) {
    let _: i32 = tc.draw(gs::integers());
    eprintln!("ran");
}
