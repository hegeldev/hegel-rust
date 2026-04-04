use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_size: usize,
    max_size: Option<usize>,
}

#[derive(Serialize)]
struct Metrics {
    length: usize,
    codepoints: Vec<u32>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test_text '<json_params>'");
        std::process::exit(1);
    }

    let params: Params = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to parse params: {}", e);
        std::process::exit(1);
    });

    Hegel::new(move |tc| {
        let mut g = gs::text().min_size(params.min_size);
        if let Some(max) = params.max_size {
            g = g.max_size(max);
        }
        let value = tc.draw(g);
        // Report length and codepoints for conformance validation
        let codepoints: Vec<u32> = value.chars().map(|c| c as u32).collect();
        let length = codepoints.len();
        write(&Metrics { length, codepoints });
    })
    .settings(Settings::new().test_cases(get_test_cases()))
    .run();
}
