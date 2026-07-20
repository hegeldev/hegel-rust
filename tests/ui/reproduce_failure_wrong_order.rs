// `#[hegel::reproduce_failure]` must appear below `#[hegel::test]`, not
// above it.

#[hegel::reproduce_failure("AAEC")]
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
