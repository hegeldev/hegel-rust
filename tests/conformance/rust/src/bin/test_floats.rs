use hegel::gen::{self, Generate};
use hegel::Hegel;
use hegel_conformance::{get_test_cases, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_value: Option<f64>,
    max_value: Option<f64>,
    exclude_min: bool,
    exclude_max: bool,
    allow_nan: bool,
    allow_infinity: bool,
}

#[derive(Serialize)]
struct Metrics {
    value: f64,
    is_nan: bool,
    is_infinite: bool,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test_floats '<json_params>'");
        std::process::exit(1);
    }

    let params: Params = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to parse params: {}", e);
        std::process::exit(1);
    });

    Hegel::new(move || {
        let mut gen = gen::floats::<f64>();

        if let Some(min) = params.min_value {
            gen = gen.with_min(min);
        }
        if let Some(max) = params.max_value {
            gen = gen.with_max(max);
        }
        if params.exclude_min {
            gen = gen.exclude_min();
        }
        if params.exclude_max {
            gen = gen.exclude_max();
        }
        gen = gen.allow_nan(params.allow_nan);
        gen = gen.allow_infinity(params.allow_infinity);

        let value = gen.generate();
        write(&Metrics {
            value,
            is_nan: value.is_nan(),
            is_infinite: value.is_infinite(),
        });
    })
    .test_cases(get_test_cases())
    .run();
}
