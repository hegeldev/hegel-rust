use hegel::generators;
use hegel::Hegel;
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
        let mut gen = generators::binary().min_size(params.min_size);
        if let Some(max) = params.max_size {
            gen = gen.max_size(max);
        }
        let value = tc.draw(gen);
        write(&Metrics {
            length: value.len(),
        });
    })
    .test_cases(get_test_cases())
    .run();
}
