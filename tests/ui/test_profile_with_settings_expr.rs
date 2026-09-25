// A positional settings expression is already a complete starting point,
// so combining it with `profile = ...` is rejected.

#[hegel::test(hegel::Settings::new(), profile = "ci")]
fn cannot_mix_profile_and_settings(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
