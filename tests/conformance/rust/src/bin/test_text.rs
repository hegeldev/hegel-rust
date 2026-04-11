use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, make_non_basic, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_size: usize,
    max_size: Option<usize>,
    codec: Option<String>,
    min_codepoint: Option<u32>,
    max_codepoint: Option<u32>,
    categories: Option<Vec<String>>,
    exclude_categories: Option<Vec<String>>,
    include_characters: Option<String>,
    exclude_characters: Option<String>,
    #[serde(default)]
    mode: String,
}

#[derive(Serialize)]
struct Metrics {
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
        if let Some(ref codec) = params.codec {
            g = g.codec(codec);
        }
        if let Some(min_codepoint) = params.min_codepoint {
            g = g.min_codepoint(min_codepoint);
        }
        if let Some(max_codepoint) = params.max_codepoint {
            g = g.max_codepoint(max_codepoint);
        }
        if let Some(ref categories) = params.categories {
            let cat_strs: Vec<&str> = categories.iter().map(|s| s.as_str()).collect();
            g = g.categories(&cat_strs);
        }
        if let Some(ref categories) = params.exclude_categories {
            let cat_strs: Vec<&str> = categories.iter().map(|s| s.as_str()).collect();
            g = g.exclude_categories(&cat_strs);
        }
        if let Some(ref chars) = params.include_characters {
            g = g.include_characters(chars);
        }
        if let Some(ref chars) = params.exclude_characters {
            g = g.exclude_characters(chars);
        }
        let value = if params.mode == "non_basic" {
            tc.draw(make_non_basic(g))
        } else {
            tc.draw(g)
        };
        let codepoints: Vec<u32> = value.chars().map(|c| c as u32).collect();
        write(&Metrics { codepoints });
    })
    .settings(Settings::new().test_cases(get_test_cases()))
    .run();
}
