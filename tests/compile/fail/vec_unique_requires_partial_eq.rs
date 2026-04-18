use hegel::generators::{self as gs, DefaultGenerator, Generator};

#[derive(hegel::DefaultGenerator)]
struct NoEq {
    value: i32,
}

fn main() {
    let _ = gs::vecs(NoEq::default_generator()).unique(true);
}
