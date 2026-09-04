// `#[hegel::test_helper]` takes no arguments.

#[hegel::test_helper(name = "x")]
fn helper(tc: &hegel::TestCase) -> bool {
    tc.draw(hegel::generators::booleans())
}

fn main() {}
