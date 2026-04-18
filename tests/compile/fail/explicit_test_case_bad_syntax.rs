// Semicolon instead of comma should produce a compile error, not a silent empty case.
#[hegel::test]
#[hegel::explicit_test_case(x = 42;)]
fn my_test(tc: hegel::TestCase) {
    let x: i32 = tc.draw(hegel::generators::integers());
    let _ = x;
}

fn main() {}
