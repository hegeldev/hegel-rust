mod common;

use common::project::TempRustProject;
use hegel::TestCase;
use hegel::generators as gs;

#[tokio::test]
#[hegel::test]
async fn test_async_with_tokio(tc: TestCase) {
    let x: bool = tc.draw(gs::booleans());
    let handle = tokio::spawn(async move { x });
    assert_eq!(handle.await.unwrap(), x);
}

#[test]
fn test_hegel_test_on_async_fn_without_wrapper_fails() {
    let code = r#"
#[hegel::test]
async fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
"#;
    TempRustProject::new()
        .main_file(code)
        .expect_failure("does not support bare interactions with async functions")
        .cargo_run(&[]);
}
