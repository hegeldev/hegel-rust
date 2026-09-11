// `#[hegel::test_helper]` needs the `TestCase` parameter to be a plain
// identifier so it can recognise draw calls on it.

#[hegel::test_helper]
fn helper(_: &hegel::TestCase) -> bool {
    true
}

fn main() {}
