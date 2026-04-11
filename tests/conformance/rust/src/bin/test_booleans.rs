use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, make_non_basic, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    #[serde(default)]
    mode: String,
}

#[derive(Serialize)]
struct Metrics {
    value: bool,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let params: Params = if args.len() >= 2 {
        serde_json::from_str(&args[1]).unwrap_or_else(|e| {
            eprintln!("Failed to parse params: {}", e);
            std::process::exit(1);
        })
    } else {
        Params {
            mode: String::new(),
        }
    };

    Hegel::new(move |tc| {
        let g = gs::booleans();
        let value = if params.mode == "non_basic" {
            tc.draw(make_non_basic(g))
        } else {
            tc.draw(g)
        };
        write(&Metrics { value });
    })
    .settings(Settings::new().test_cases(get_test_cases()))
    .run();
}
