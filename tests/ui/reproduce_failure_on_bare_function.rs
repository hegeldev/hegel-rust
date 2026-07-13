// `#[hegel::reproduce_failure]` can only be used together with
// `#[hegel::test]`.

#[hegel::reproduce_failure("AAEC")]
fn my_func(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
