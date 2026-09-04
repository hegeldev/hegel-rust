use hegel::DefaultGenerator;
use hegel::generators::{self as gs, Generator};

#[derive(Debug, DefaultGenerator)]
struct Config {
    name: String,
}

struct SilentText;

impl Generator<String> for SilentText {
    fn do_draw(&self, tc: &hegel::TestCase) -> String {
        tc.draw_silent(gs::text())
    }
}

#[hegel::test]
fn draws_with_a_silent_field_generator(tc: hegel::TestCase) {
    let generator = gs::default::<Config>().name(SilentText);
    let _: Config = tc.draw(generator);
}

fn main() {}
