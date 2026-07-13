// A field named `new` collides with the generated builder API and must be a
// clean compile error.

#[derive(Debug, hegel::DefaultGenerator)]
struct Odd {
    new: bool,
}

fn main() {}
