//! Fixture binary for `tests/test_fuzz_client.rs`: a `#[hegel::main]` entry
//! point whose test case draws once and then sleeps, so a fuzz server test
//! has time to kill the case it asked for.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main]
fn main(tc: TestCase) {
    let _: i32 = tc.draw(gs::integers());
    std::thread::sleep(std::time::Duration::from_secs(30));
}
