// A syntax error inside the explicit-test-case argument list (here a `;`
// where only `,` is allowed) is reported by the macro's parser.

#[hegel::test]
#[hegel::explicit_test_case(x = 42;)]
fn my_test(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
