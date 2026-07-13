// `#[hegel::test]` does not support bare interactions with async functions;
// an async runtime wrapper (e.g. `#[tokio::test]`) must sit above it.

#[hegel::test]
async fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
