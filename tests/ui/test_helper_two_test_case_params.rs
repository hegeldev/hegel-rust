// `#[hegel::test_helper]` cannot tell which of two `TestCase` parameters
// owns the draws.

#[hegel::test_helper]
fn helper(_tc: &hegel::TestCase, _other: &hegel::TestCase) -> bool {
    true
}

fn main() {}
