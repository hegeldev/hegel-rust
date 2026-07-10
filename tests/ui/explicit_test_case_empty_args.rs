// `#[hegel::explicit_test_case()]` with an empty argument list is rejected:
// an explicit case requires at least one value.

#[hegel::test]
#[hegel::explicit_test_case()]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
