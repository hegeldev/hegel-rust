// `#[hegel::standalone_function]` cannot be combined with `#[test]`.

#[hegel::standalone_function]
#[test]
fn bad(tc: hegel::TestCase) {
    let _ = tc;
}

fn main() {}
