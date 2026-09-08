//! Fixture binary for `tests/test_hegel_main.rs`: a `#[hegel::main]` entry
//! point whose single test case makes far more choices than the engine's
//! default per-case bound allows, so the driver tests can check that a main
//! binary runs such a case to completion instead of cutting it off as an
//! overrun.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main]
fn main(tc: TestCase) {
    let mut trues = 0;
    for _ in 0..20_000 {
        if tc.draw(gs::booleans()) {
            trues += 1;
        }
    }
    eprintln!("ran with {trues} trues");
}
