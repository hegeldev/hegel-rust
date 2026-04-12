use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, maybe_non_basic, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_size: usize,
    max_size: Option<usize>,
    min_value: Option<i32>,
    max_value: Option<i32>,
    mode: String,
    unique: bool,
}

#[derive(Serialize)]
struct Metrics {
    elements: Vec<i32>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test_lists '<json_params>'");
        std::process::exit(1);
    }

    let params: Params = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to parse params: {}", e);
        std::process::exit(1);
    });

    Hegel::new(move |tc| {
        let mut g = gs::integers::<i32>();
        if let Some(min) = params.min_value {
            g = g.min_value(min);
        }
        if let Some(max) = params.max_value {
            g = g.max_value(max);
        }

        let mut vec_gen = gs::vecs(maybe_non_basic(g, &params.mode))
            .min_size(params.min_size)
            .unique(params.unique);
        if let Some(max) = params.max_size {
            vec_gen = vec_gen.max_size(max);
        }

        let list = tc.draw(vec_gen);
        write(&Metrics { elements: list });
    })
    .settings(Settings::new().test_cases(get_test_cases()))
    .run();
}
