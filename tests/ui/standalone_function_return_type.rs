// `#[hegel::standalone_function]` functions must not have a return type.

#[hegel::standalone_function]
fn bad(tc: hegel::TestCase) -> i32 {
    let _ = tc;
    0
}

fn main() {}
