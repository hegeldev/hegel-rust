use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, make_non_basic, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    options: Vec<i32>,
    #[serde(default)]
    mode: String,
}

#[derive(Serialize)]
struct Metrics {
    value: i32,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test_sampled_from '<json_params>'");
        std::process::exit(1);
    }

    let params: Params = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to parse params: {}", e);
        std::process::exit(1);
    });

    Hegel::new(move |tc| {
        let g = gs::sampled_from(params.options.clone());
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
