//! Fixture binary: a bare `#[hegel::main]` (no attribute arguments) drawing a
//! single boolean. Drivers use it for `--help`, `--seed`, and `--verbosity`
//! CLI handling.

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main]
fn main(tc: TestCase) {
    let _: bool = tc.draw(gs::booleans());
}
