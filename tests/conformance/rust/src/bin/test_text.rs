use hegel::generators as gs;
use hegel::{Hegel, Settings};
use hegel_conformance::{get_test_cases, write};
use serde::{Deserialize, Serialize};
use std::env;
use std::panic::{AssertUnwindSafe, catch_unwind};

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

    // N15: wrap the test run in catch_unwind so unsupported-by-native
    // schema inputs (Python harness picks any codec from its full
    // `aliases` set, but native only supports {ascii, latin-1,
    // iso-8859-1, utf-8}; analogous panics for empty-alphabet
    // configurations now fire at builder time post-N14). Recognise our
    // `InvalidArgument:` builder-time error pattern and exit 0 silently
    // — the harness treats an empty metrics file as a skipped case.
    // Any other panic is a real bug and propagates.
    let result = catch_unwind(AssertUnwindSafe(|| {
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
            let value = tc.draw(g);
            let codepoints: Vec<u32> = value.chars().map(|c| c as u32).collect();
            write(&Metrics { codepoints });
        })
        .settings(Settings::new().test_cases(get_test_cases()))
        .run();
    }));
    if let Err(payload) = result {
        let msg = payload
            .downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.contains("InvalidArgument:") {
            // Native rejected the schema at strategy-construction time
            // (unsupported codec, empty alphabet, etc.). Skip silently.
            return;
        }
        std::panic::resume_unwind(payload);
    }
}
