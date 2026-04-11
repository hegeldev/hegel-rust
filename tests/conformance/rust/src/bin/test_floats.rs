use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_value: Option<f64>,
    max_value: Option<f64>,
    exclude_min: bool,
    exclude_max: bool,
    allow_nan: Option<bool>,
    allow_infinity: Option<bool>,
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

    Hegel::new(move |tc| {
        let mut g = gs::floats::<f64>();

        if let Some(min) = params.min_value {
            g = g.min_value(min);
        }
        if let Some(max) = params.max_value {
            g = g.max_value(max);
        }
        g = g.exclude_min(params.exclude_min);
        g = g.exclude_max(params.exclude_max);
        if let Some(allow_nan) = params.allow_nan {
            g = g.allow_nan(allow_nan);
        }
        if let Some(allow_infinity) = params.allow_infinity {
            g = g.allow_infinity(allow_infinity);
        }

        let value = tc.draw(g);
        write(&Metrics {
            value,
            is_nan: value.is_nan(),
            is_infinite: value.is_infinite(),
        });
    })
    .settings(Settings::new().test_cases(get_test_cases()))
    .run();
}
