use hegel::generators;
use hegel::Hegel;
use hegel_conformance::{get_test_cases, write};
use serde::Serialize;

#[derive(Serialize)]
struct Metrics {
    value: bool,
}

fn main() {
    // booleans takes no params, so we ignore argv[1]

    Hegel::new(|tc| {
        let value = tc.draw(generators::booleans());
        write(&Metrics { value });
    })
    .test_cases(get_test_cases())
    .run();
}
