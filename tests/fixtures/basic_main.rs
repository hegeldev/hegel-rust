//! Fixture binary for `tests/test_hegel_main.rs`: a `#[hegel::main]` entry
//! point that logs each execution of the body, so the driver tests can
//! check that a main binary runs exactly one test case and rejects
//! unknown arguments, against a real prebuilt binary.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main]
fn main(tc: TestCase) {
    let _: i32 = tc.draw(gs::integers());
    eprintln!("ran");
}
