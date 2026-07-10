// `#[hegel::explicit_test_case]` without an argument list is rejected: the
// attribute requires arguments.

#[hegel::test]
#[hegel::explicit_test_case]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
