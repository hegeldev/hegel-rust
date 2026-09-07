// `#[hegel::test_helper]` requires a `TestCase` parameter to rewrite draws
// against.

#[hegel::test_helper]
fn helper(len: usize) -> usize {
    len
}

fn main() {}
