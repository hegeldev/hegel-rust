use hegel::TestCase;
use hegel::generators::integers;
use hegel_macros::Generator;

#[derive(Debug, Generator)]
struct Person {
    name: String,
    age: i32,
}
