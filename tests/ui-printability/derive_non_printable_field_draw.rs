use hegel::DefaultGenerator;
use hegel::generators::{self as gs, Generator};

#[derive(Debug, DefaultGenerator)]
struct Config {
    name: String,
}

#[hegel::test]
fn draws_with_a_silent_field_generator(tc: hegel::TestCase) {
    let generator = gs::default::<Config>().name(gs::text().boxed());
    let _: Config = tc.draw(generator);
}

fn main() {}
