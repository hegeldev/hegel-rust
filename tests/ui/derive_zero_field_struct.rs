// `DefaultGenerator` cannot be derived for structs with no fields — unit,
// empty named, or empty tuple: there is nothing to configure.

#[derive(Debug, hegel::DefaultGenerator)]
struct Unit;

#[derive(Debug, hegel::DefaultGenerator)]
struct EmptyNamed {}

#[derive(Debug, hegel::DefaultGenerator)]
struct EmptyTuple();

fn main() {}
