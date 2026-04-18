#[hegel::explicit_test_case(x = 42)]
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
