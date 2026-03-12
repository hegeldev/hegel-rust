use hegel::generators;
use hegel::Hegel;
use hegel_conformance::{get_test_cases, write};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

#[derive(Deserialize)]
struct Params {
    min_size: usize,
    max_size: usize,
    key_type: String,
    min_key: i32,
    max_key: i32,
    min_value: i32,
    max_value: i32,
}

#[derive(Serialize)]
struct Metrics {
    size: usize,
    min_key: Option<i32>,
    max_key: Option<i32>,
    min_value: Option<i32>,
    max_value: Option<i32>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test_hashmaps '<json_params>'");
        std::process::exit(1);
    }

    let params: Params = serde_json::from_str(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to parse params: {}", e);
        std::process::exit(1);
    });

    Hegel::new(move |tc| {
        let size: usize;
        let min_key: Option<i32>;
        let max_key: Option<i32>;
        let min_value: Option<i32>;
        let max_value: Option<i32>;

        match params.key_type.as_str() {
            "integer" => {
                let hashmap_gen = generators::hashmaps(
                    generators::integers::<i32>()
                        .min_value(params.min_key)
                        .max_value(params.max_key),
                    generators::integers::<i32>()
                        .min_value(params.min_value)
                        .max_value(params.max_value),
                )
                .min_size(params.min_size)
                .max_size(params.max_size);

                let map = tc.draw(hashmap_gen);
                size = map.len();
                if map.is_empty() {
                    min_key = None;
                    max_key = None;
                    min_value = None;
                    max_value = None;
                } else {
                    min_key = map.keys().min().copied();
                    max_key = map.keys().max().copied();
                    min_value = map.values().min().copied();
                    max_value = map.values().max().copied();
                }
            }
            "string" => {
                let hashmap_gen = generators::hashmaps(
                    generators::text(),
                    generators::integers::<i32>()
                        .min_value(params.min_value)
                        .max_value(params.max_value),
                )
                .min_size(params.min_size)
                .max_size(params.max_size);

                let map = tc.draw(hashmap_gen);
                size = map.len();
                min_key = None;
                max_key = None;
                if map.is_empty() {
                    min_value = None;
                    max_value = None;
                } else {
                    min_value = map.values().min().copied();
                    max_value = map.values().max().copied();
                }
            }
            _ => {
                eprintln!("Unknown key_type: {}", params.key_type);
                std::process::exit(1);
            }
        }

        write(&Metrics {
            size,
            min_key,
            max_key,
            min_value,
            max_value,
        });
    })
    .test_cases(get_test_cases())
    .run();
}
