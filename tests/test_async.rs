mod common;

use hegel::TestCase;
use hegel::generators as gs;

#[tokio::test]
#[hegel::test]
async fn test_async_with_tokio(tc: TestCase) {
    let x: bool = tc.draw(gs::booleans());
    let handle = tokio::spawn(async move { x });
    assert_eq!(handle.await.unwrap(), x);
}

// `test_hegel_test_on_async_fn_without_wrapper_fails` lives in
// `tests/ui/hegel_test_on_bare_async_fn.rs`: rejecting a bare async fn is a
// compile-time diagnostic, pinned by trybuild.
