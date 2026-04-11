use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, write};
use serde::Serialize;

#[derive(Serialize)]
struct Metrics {
    value: bool,
}

fn main() {
    Hegel::new(move |tc| {
        let value = tc.draw(gs::booleans());
        write(&Metrics { value });
    })
    .settings(Settings::new().test_cases(get_test_cases()))
    .run();
}
