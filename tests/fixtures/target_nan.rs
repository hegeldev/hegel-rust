use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main]
fn main(tc: TestCase) {
    let n: u32 = tc.draw(gs::integers().max_value(0));
    tc.target(1.0 / n as f64);
}
