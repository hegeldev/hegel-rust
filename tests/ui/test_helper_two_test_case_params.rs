// `#[hegel::test_helper]` cannot tell which of two `TestCase` parameters
// owns the draws.

#[hegel::test_helper]
fn helper(tc: &hegel::TestCase, other: &hegel::TestCase) -> bool {
    let _ = other;
    tc.draw(hegel::generators::booleans())
}

fn main() {}
