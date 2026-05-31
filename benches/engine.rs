//! End-to-end benchmarks for the native engine.
//!
//! Unlike `biased_sample.rs`, which microbenchmarks individual
//! `biased_*_sample` calls, these drive *whole* property-test runs through the
//! public [`Hegel`] API — the same generate-and-shrink path the integration
//! suites (`test_integers`, `test_collections`, `test_quality`,
//! `test_shrink_quality`) exercise. They're the right yardstick for changes to
//! the choice/shrink machinery (e.g. `IntegerChoice::from_index`), where a
//! micro win can wash out once inlining and the surrounding work are included.
//!
//! Run with:
//!
//! ```text
//! cargo bench --features native --bench engine
//! ```
//!
//! Two groups:
//! - `generation`: a property that always holds, so the engine runs the full
//!   example budget without shrinking — pure draw throughput.
//! - `shrinking`: a property that fails, forcing the engine to find a
//!   counterexample and minimise it — the (usually dominant) shrink path,
//!   driven exactly like the `Minimal` test helper.
//!
//! Every run is pinned to a fixed seed, so each Criterion iteration repeats the
//! same work and A/B comparisons (e.g. before/after a choice-arithmetic change)
//! are meaningful.

use std::collections::HashSet;
use std::hint::black_box;
use std::panic::{AssertUnwindSafe, catch_unwind};

use criterion::{Criterion, criterion_group, criterion_main};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings, TestCase};

// Fixed seed so the generated sequence — and therefore the measured work — is
// identical across iterations and across A/B builds.
const SEED: u64 = 0xC0FFEE;

// ── generation throughput ───────────────────────────────────────────────────
//
// The property always holds, so the engine generates and discards
// `GEN_EXAMPLES` test cases with no shrinking. This isolates the draw path:
// `from_index` / `biased_*_sample` / span bookkeeping.

const GEN_EXAMPLES: u64 = 1000;

fn gen_settings() -> Settings {
    Settings::new()
        .test_cases(GEN_EXAMPLES)
        .seed(Some(SEED))
        .database(None)
}

fn run_generation(test: impl FnMut(TestCase) + Send + 'static) {
    Hegel::new(test).settings(gen_settings()).run();
}

fn bench_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("generation");
    // Each iteration is a full GEN_EXAMPLES-case run (several ms); keep the
    // sample count modest so the group finishes in reasonable wall-clock time.
    group.sample_size(20);

    group.bench_function("integers_i64", |b| {
        b.iter(|| {
            run_generation(|tc| {
                black_box(tc.draw(gs::integers::<i64>()));
            })
        });
    });

    group.bench_function("integers_bounded", |b| {
        b.iter(|| {
            run_generation(|tc| {
                let g = gs::integers::<i64>()
                    .min_value(-1_000_000)
                    .max_value(1_000_000);
                black_box(tc.draw(g));
            })
        });
    });

    group.bench_function("vec_i64", |b| {
        b.iter(|| {
            run_generation(|tc| {
                black_box(tc.draw(gs::vecs(gs::integers::<i64>()).max_size(32)));
            })
        });
    });

    group.bench_function("strings", |b| {
        b.iter(|| {
            run_generation(|tc| {
                black_box(tc.draw(gs::text().max_size(32)));
            })
        });
    });

    group.bench_function("hash_map", |b| {
        b.iter(|| {
            run_generation(|tc| {
                let g = gs::hashmaps(gs::integers::<i64>(), gs::text()).max_size(16);
                black_box(tc.draw(g));
            })
        });
    });

    group.bench_function("nested_vec_tuples", |b| {
        b.iter(|| {
            run_generation(|tc| {
                let g = gs::vecs(gs::tuples!(gs::integers::<i64>(), gs::text())).max_size(16);
                black_box(tc.draw(g));
            })
        });
    });

    // flat_map: a length drawn first, then a vec of that length. Exercises the
    // compositional (non-basic) draw path with nested spans.
    group.bench_function("flat_map_vec", |b| {
        b.iter(|| {
            run_generation(|tc| {
                let g = gs::integers::<i64>()
                    .min_value(0)
                    .max_value(16)
                    .flat_map(|n: i64| {
                        gs::vecs(gs::integers::<i64>())
                            .min_size(n as usize)
                            .max_size(n as usize)
                    });
                black_box(tc.draw(g));
            })
        });
    });

    // Targeting: each case reports a score via `tc.target`, driving the
    // targeting phase's bookkeeping in addition to plain generation.
    group.bench_function("targeting", |b| {
        b.iter(|| {
            run_generation(|tc| {
                let v: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()).max_size(32));
                tc.target(v.iter().sum::<i64>() as f64);
            })
        });
    });

    group.finish();
}

// ── shrinking ───────────────────────────────────────────────────────────────
//
// These mirror the `Minimal` test helper: any value satisfying the predicate
// makes the test "fail", so the engine finds an interesting example and then
// minimises it. The work is dominated by the shrink passes, which hammer
// `to_index`/`from_index`, `sort_key`, and the per-choice bignum bookkeeping.
// `catch_unwind` swallows the resulting failure panic (the engine reports a
// failing property by panicking out of `run()`).

const SHRINK_EXAMPLES: u64 = 500;

fn shrink_settings() -> Settings {
    Settings::new()
        .test_cases(SHRINK_EXAMPLES)
        .seed(Some(SEED))
        .database(None)
}

/// Drive a full find-and-shrink run, discarding the failure panic. `predicate`
/// returns `true` for an "interesting" (counterexample) test case.
fn run_shrink(predicate: impl Fn(TestCase) -> bool + Send + Sync + 'static) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(move |tc| {
            if predicate(tc) {
                panic!("HEGEL_BENCH_FOUND");
            }
        })
        .settings(shrink_settings())
        .run();
    }));
}

fn bench_shrinking(c: &mut Criterion) {
    // Silence the engine's failure reporting + panic backtraces; every shrink
    // iteration deliberately fails. (The bench binary does nothing else, so a
    // process-wide no-op hook is fine here.)
    std::panic::set_hook(Box::new(|_| {}));

    let mut group = c.benchmark_group("shrinking");
    // Shrink runs are the heaviest single iterations here.
    group.sample_size(10);

    // Integer boundary: shrinks toward the smallest x >= 1_000_000.
    group.bench_function("integer_boundary", |b| {
        b.iter(|| {
            run_shrink(|tc| {
                let x: i64 = tc.draw(gs::integers::<i64>());
                x >= 1_000_000
            })
        });
    });

    // List whose sum crosses a threshold: shrinks toward `[1000]`.
    group.bench_function("vec_sum_threshold", |b| {
        b.iter(|| {
            run_shrink(|tc| {
                let v: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()));
                v.iter().sum::<i64>() >= 1000
            })
        });
    });

    // List containing a duplicate: shrinks toward `[0, 0]`.
    group.bench_function("vec_duplicates", |b| {
        b.iter(|| {
            run_shrink(|tc| {
                let v: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()));
                let mut seen = HashSet::new();
                v.iter().any(|x| !seen.insert(*x))
            })
        });
    });

    // Nested structure: a list of lists with a large total element count.
    group.bench_function("nested_vec_of_vecs", |b| {
        b.iter(|| {
            run_shrink(|tc| {
                let v: Vec<Vec<i64>> = tc.draw(gs::vecs(gs::vecs(gs::integers::<i64>())));
                v.iter().map(|inner| inner.len()).sum::<usize>() >= 8
            })
        });
    });

    // String reaching a target length: exercises the string choice + shrink.
    group.bench_function("string_length", |b| {
        b.iter(|| {
            run_shrink(|tc| {
                let s: String = tc.draw(gs::text());
                s.chars().count() >= 5
            })
        });
    });

    group.finish();
}

criterion_group!(benches, bench_generation, bench_shrinking);
criterion_main!(benches);
