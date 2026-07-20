// `#[hegel::explicit_test_case]` can only be used together with
// `#[hegel::test]` (or `#[hegel::main]` / `#[hegel::standalone_function]`).

#[hegel::explicit_test_case(x = 42)]
fn my_func(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
