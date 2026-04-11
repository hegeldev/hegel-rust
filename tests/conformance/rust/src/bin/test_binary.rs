use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, make_non_basic, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_size: usize,
    max_size: Option<usize>,
    #[serde(default)]
    mode: String,
}

#[derive(Serialize)]
struct Metrics {
    length: usize,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test_binary '<json_params>'");
        std::process::exit(1);
    }

    let params: Params = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to parse params: {}", e);
        std::process::exit(1);
    });

    Hegel::new(move |tc| {
        let mut g = gs::binary().min_size(params.min_size);
        if let Some(max) = params.max_size {
            g = g.max_size(max);
        }
        let value = if params.mode == "non_basic" {
            tc.draw(make_non_basic(g))
        } else {
            tc.draw(g)
        };
        write(&Metrics {
            length: value.len(),
        });
    })
    .settings(Settings::new().test_cases(get_test_cases()))
    .run();
}
