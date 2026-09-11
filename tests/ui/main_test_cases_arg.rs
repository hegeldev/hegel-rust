// `#[hegel::main]` binaries always run a single test case, so the
// `test_cases` attribute argument is rejected.

#[hegel::main(test_cases = 5)]
fn main(tc: hegel::TestCase) {
    let _ = tc;
}
