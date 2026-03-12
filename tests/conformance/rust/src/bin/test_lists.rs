use hegel::generators;
use hegel::Hegel;
use hegel_conformance::{get_test_cases, write};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Deserialize)]
struct Params {
    min_size: usize,
    max_size: Option<usize>,
    min_value: Option<i32>,
    max_value: Option<i32>,
}

#[derive(Serialize)]
struct Metrics {
    size: usize,
    min_element: Option<i32>,
    max_element: Option<i32>,
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
        let mut elem_gen = generators::integers::<i32>();
        if let Some(min) = params.min_value {
            elem_gen = elem_gen.min_value(min);
        }
        if let Some(max) = params.max_value {
            elem_gen = elem_gen.max_value(max);
        }

        let mut vec_gen = generators::vecs(elem_gen).min_size(params.min_size);
        if let Some(max) = params.max_size {
            vec_gen = vec_gen.max_size(max);
        }

        let list = tc.draw(vec_gen);

        let size = list.len();
        let (min_element, max_element) = if list.is_empty() {
            (None, None)
        } else {
            (list.iter().min().copied(), list.iter().max().copied())
        };

        write(&Metrics {
            size,
            min_element,
            max_element,
        });
    })
    .test_cases(get_test_cases())
    .run();
}
