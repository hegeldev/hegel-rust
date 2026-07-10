//! Fixture binary: `#[hegel::main]` rewrites `let my_var = tc.draw(…)` so the
//! failure report names the drawn variable (`let my_var = …;`).

use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main(test_cases = 1)]
fn main(tc: TestCase) {
    let my_var: i32 = tc.draw(gs::integers());
    panic!("boom {}", my_var);
}
