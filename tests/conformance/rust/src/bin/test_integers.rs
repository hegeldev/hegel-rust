use hegel::generators;
use hegel::Hegel;
use hegel_conformance::{get_test_cases, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_value: Option<i32>,
    max_value: Option<i32>,
}

#[derive(Serialize)]
struct Metrics {
    value: i32,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test_integers '<json_params>'");
        std::process::exit(1);
    }

    let params: Params = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to parse params: {}", e);
        std::process::exit(1);
    });

    Hegel::new(move |tc| {
        let mut generator = generators::integers::<i32>();
        if let Some(min) = params.min_value {
            generator = generator.min_value(min);
        }
        if let Some(max) = params.max_value {
            generator = generator.max_value(max);
        }
        let value = tc.draw(generator);
        write(&Metrics { value });
    })
    .test_cases(get_test_cases())
    .run();
}
