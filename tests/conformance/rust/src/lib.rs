//! Metrics utilities for conformance tests.

use hegel::generators::{BoxedGenerator, Generator};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;

static METRICS_FILE: OnceLock<Option<std::fs::File>> = OnceLock::new();

fn get_metrics_file() -> &'static Option<std::fs::File> {
    METRICS_FILE.get_or_init(|| {
        std::env::var("CONFORMANCE_METRICS_FILE")
            .ok()
            .and_then(|path| OpenOptions::new().append(true).create(true).open(path).ok())
    })
}

/// Get the number of test cases to run from environment variable.
pub fn get_test_cases() -> u64 {
    std::env::var("CONFORMANCE_TEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
}

/// Wrap a generator in a trivial composite so `as_basic()` returns `None`,
/// forcing the compositional fallback path.
pub fn make_non_basic<T: std::fmt::Debug + 'static>(
    generator: impl Generator<T> + 'static,
) -> BoxedGenerator<'static, T> {
    hegel::compose!(|tc| { tc.draw(&generator) }).boxed()
}

/// Write metrics as a JSON line to the metrics file.
pub fn write<T: Serialize>(metrics: &T) {
    // We need interior mutability for the file handle
    if let Some(ref file) = *get_metrics_file() {
        // Clone the file handle to get a mutable reference
        let mut file = file.try_clone().unwrap();
        let json = serde_json::to_string(metrics).unwrap();
        writeln!(file, "{}", json).unwrap();
    }
}
