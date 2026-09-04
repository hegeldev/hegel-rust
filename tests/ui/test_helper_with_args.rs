// `#[hegel::test_helper]` takes no arguments.

#[hegel::test_helper(name = "x")]
fn helper(_tc: &hegel::TestCase) -> bool {
    true
}

fn main() {}
