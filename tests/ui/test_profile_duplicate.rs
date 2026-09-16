// `profile` may be given at most once.

#[hegel::test(profile = "ci", profile = "default")]
fn duplicate_profile(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
