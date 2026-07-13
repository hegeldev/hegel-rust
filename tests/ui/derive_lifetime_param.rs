// `DefaultGenerator` does not support lifetime parameters: generated values
// must be owned.

#[derive(Debug, hegel::DefaultGenerator)]
struct Borrowed<'a> {
    x: &'a str,
}

fn main() {}
