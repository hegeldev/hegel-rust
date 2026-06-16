use hegel::TestCase;
use hegel::generators as gs;

#[hegel::main]
fn main(tc: TestCase) {
    let n = tc.draw(gs::integers::<u64>());

    let edge = 1337;
    assert!(n != edge);
    assert!(n < edge);
}
