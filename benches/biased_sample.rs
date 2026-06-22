//! Microbenchmarks for the `biased_*_sample` functions on the native engine.
//!
//! Run with:
//!
//! ```text
//! cargo bench --features __bench --bench biased_sample
//! ```
//!
//! These functions sit on the hottest path of native test-case generation
//! (`data_tree::pick_non_exhausted_value` → `ChoiceKind::random_value` → here),
//! so even small per-call wins compound across a full property-test run.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

use hegel::__bench::{
    BytesChoice, EngineRng, FloatChoice, IntegerChoice, IntervalSet, StringChoice,
    biased_bytes_sample, biased_float_sample, biased_integer_sample, biased_string_sample,
};

type BigInt = hegel::__bench::BigInt;

fn integer_cases() -> Vec<(&'static str, IntegerChoice)> {
    vec![
        (
            "i64_unbounded",
            IntegerChoice {
                min_value: BigInt::from(i64::MIN),
                max_value: BigInt::from(i64::MAX),
                shrink_towards: BigInt::from(0),
            },
        ),
        (
            "i32_unbounded",
            IntegerChoice {
                min_value: BigInt::from(i32::MIN),
                max_value: BigInt::from(i32::MAX),
                shrink_towards: BigInt::from(0),
            },
        ),
        (
            "small_window_0_100",
            IntegerChoice {
                min_value: BigInt::from(0),
                max_value: BigInt::from(100),
                shrink_towards: BigInt::from(0),
            },
        ),
        (
            "tight_window_42_43",
            IntegerChoice {
                min_value: BigInt::from(42),
                max_value: BigInt::from(43),
                shrink_towards: BigInt::from(42),
            },
        ),
    ]
}

fn bench_biased_integer_sample(c: &mut Criterion) {
    let mut group = c.benchmark_group("biased_integer_sample");
    group.throughput(Throughput::Elements(1));
    for (name, ic) in integer_cases() {
        group.bench_with_input(BenchmarkId::from_parameter(name), &ic, |b, ic| {
            let mut rng = EngineRng::seeded(0xC0FFEE);
            b.iter(|| black_box(biased_integer_sample(black_box(ic), &mut rng)));
        });
    }
    group.finish();
}

fn ascii_string_choice() -> StringChoice {
    StringChoice {
        intervals: IntervalSet::new(vec![(0x20, 0x7E)]),
        min_size: 0,
        max_size: 100,
    }
}

fn unicode_string_choice() -> StringChoice {
    StringChoice {
        intervals: IntervalSet::new(vec![(0x0, 0xD7FF), (0xE000, 0x10FFFF)]),
        min_size: 0,
        max_size: 100,
    }
}

fn bench_biased_string_sample(c: &mut Criterion) {
    let mut group = c.benchmark_group("biased_string_sample");
    group.throughput(Throughput::Elements(1));
    let ascii = ascii_string_choice();
    let unicode = unicode_string_choice();
    group.bench_function("ascii_0_100", |b| {
        let mut rng = EngineRng::seeded(0xC0FFEE);
        b.iter(|| black_box(biased_string_sample(black_box(&ascii), &mut rng)));
    });
    group.bench_function("unicode_0_100", |b| {
        let mut rng = EngineRng::seeded(0xC0FFEE);
        b.iter(|| black_box(biased_string_sample(black_box(&unicode), &mut rng)));
    });
    group.finish();
}

fn bench_biased_bytes_sample(c: &mut Criterion) {
    let mut group = c.benchmark_group("biased_bytes_sample");
    group.throughput(Throughput::Elements(1));
    let bc = BytesChoice {
        min_size: 0,
        max_size: 100,
    };
    group.bench_function("0_100", |b| {
        let mut rng = EngineRng::seeded(0xC0FFEE);
        b.iter(|| black_box(biased_bytes_sample(black_box(&bc), &mut rng)));
    });
    group.finish();
}

fn bench_biased_float_sample(c: &mut Criterion) {
    let mut group = c.benchmark_group("biased_float_sample");
    group.throughput(Throughput::Elements(1));
    let unbounded = FloatChoice {
        min_value: f64::NEG_INFINITY,
        max_value: f64::INFINITY,
        allow_nan: true,
        allow_infinity: true,
        smallest_nonzero_magnitude: 5e-324,
    };
    let bounded = FloatChoice {
        min_value: -1.0,
        max_value: 1.0,
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: 5e-324,
    };
    group.bench_function("unbounded", |b| {
        let mut rng = EngineRng::seeded(0xC0FFEE);
        b.iter(|| black_box(biased_float_sample(black_box(&unbounded), &mut rng)));
    });
    group.bench_function("bounded_pm1", |b| {
        let mut rng = EngineRng::seeded(0xC0FFEE);
        b.iter(|| black_box(biased_float_sample(black_box(&bounded), &mut rng)));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_biased_integer_sample,
    bench_biased_string_sample,
    bench_biased_bytes_sample,
    bench_biased_float_sample,
);
criterion_main!(benches);
