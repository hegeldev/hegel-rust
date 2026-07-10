// `DefaultGenerator` cannot be derived for enums with no variants: there is
// nothing to generate.

#[derive(Debug, hegel::DefaultGenerator)]
enum Void {}

fn main() {}
