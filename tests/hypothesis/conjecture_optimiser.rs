//! Ported from hypothesis-python/tests/conjecture/test_optimiser.py
//!
//! Every test in this file exercises Hypothesis's targeted property-based
//! testing machinery — `data.target_observations`, `runner.optimise_targets()`,
//! and `runner.best_observed_targets` — via the native surface exposed
//! through `hegel::__native_test_internals::TargetedRunner` /
//! `TargetedTestCase`. Tests are native-gated so the test binary still
//! compiles and links under both `--features native` and the default
//! (server-only) build.

#![cfg(feature = "native")]

use hegel::__native_test_internals::{
    BigUint, BooleanChoice, BufferSizeLimit, BytesChoice, ChoiceKind, ChoiceValue, FloatChoice,
    IntegerChoice, IntervalSet, RunIsComplete, Status, StringChoice, TargetedRunner,
    TargetedRunnerSettings, TargetedTestCase, compute_max_children,
};
use hegel::TestCase;
use hegel::generators::{self as gs};
use rand::SeedableRng;
use rand::rngs::SmallRng;

fn runner_settings() -> TargetedRunnerSettings {
    TargetedRunnerSettings::new().max_examples(100)
}

fn rng() -> SmallRng {
    SmallRng::seed_from_u64(0)
}

fn observe(data: &mut TargetedTestCase, key: &str, value: f64) {
    data.target_observations.insert(key.to_string(), value);
}

fn ignore_run_is_complete<T>(r: Result<T, RunIsComplete>) {
    match r {
        Ok(_) | Err(RunIsComplete) => {}
    }
}

fn best(runner: &TargetedRunner, key: &str) -> f64 {
    *runner.best_observed_targets().get(key).unwrap_or_else(|| {
        panic!(
            "no best_observed_targets entry for {key:?}; have {:?}",
            runner.best_observed_targets().keys().collect::<Vec<_>>()
        )
    })
}

fn integer_choices(xs: &[i128]) -> Vec<ChoiceValue> {
    xs.iter().map(|&x| ChoiceValue::Integer(x)).collect()
}

#[test]
fn test_optimises_to_maximum() {
    let mut runner = TargetedRunner::new(
        |data| {
            let m = data.draw_integer(0, (1i128 << 8) - 1);
            observe(data, "m", m as f64);
        },
        runner_settings(),
        rng(),
    );
    runner.cached_test_function(&integer_choices(&[0]));
    ignore_run_is_complete(runner.optimise_targets());
    assert_eq!(best(&runner, "m"), 255.0);
}

#[test]
fn test_optimises_multiple_targets() {
    let mut runner = TargetedRunner::new(
        |data| {
            let n = data.draw_integer(0, (1i128 << 8) - 1);
            let m = data.draw_integer(0, (1i128 << 8) - 1);
            if n + m > 256 {
                data.mark_invalid();
            }
            observe(data, "m", m as f64);
            observe(data, "n", n as f64);
            observe(data, "m + n", (m + n) as f64);
        },
        runner_settings(),
        rng(),
    );
    runner.cached_test_function(&integer_choices(&[200, 0]));
    runner.cached_test_function(&integer_choices(&[0, 200]));
    ignore_run_is_complete(runner.optimise_targets());
    assert_eq!(best(&runner, "m"), 255.0);
    assert_eq!(best(&runner, "n"), 255.0);
    assert_eq!(best(&runner, "m + n"), 256.0);
}

#[test]
fn test_optimises_when_last_element_is_empty() {
    let mut runner = TargetedRunner::new(
        |data| {
            let n = data.draw_integer(0, (1i128 << 8) - 1);
            observe(data, "n", n as f64);
            data.start_span(1);
            data.stop_span();
        },
        runner_settings(),
        rng(),
    );
    runner.cached_test_function(&integer_choices(&[250]));
    ignore_run_is_complete(runner.optimise_targets());
    assert_eq!(best(&runner, "n"), 255.0);
}

#[test]
fn test_can_optimise_last_with_following_empty() {
    let mut runner = TargetedRunner::new(
        |data| {
            for _ in 0..100 {
                data.draw_integer(0, 3);
            }
            let v = data.draw_integer(0, (1i128 << 8) - 1);
            observe(data, "", v as f64);
            data.start_span(1);
            data.stop_span();
        },
        runner_settings().max_examples(100),
        rng(),
    );
    let choices: Vec<ChoiceValue> = (0..101).map(|_| ChoiceValue::Integer(0)).collect();
    runner.cached_test_function(&choices);
    match runner.optimise_targets() {
        Err(RunIsComplete) => {}
        Ok(_) => panic!("expected RunIsComplete"),
    }
    assert_eq!(best(&runner, ""), 255.0);
}

fn check_find_endpoints(lower: i128, upper: i128, score_up: bool) {
    let mut runner = TargetedRunner::new(
        move |data| {
            let n = data.draw_integer(0, (1i128 << 16) - 1);
            if n < lower || n > upper {
                data.mark_invalid();
            }
            let scored = if score_up { n } else { -n };
            observe(data, "n", scored as f64);
        },
        runner_settings().max_examples(1000),
        rng(),
    );
    let start = (lower + upper) / 2;
    runner.cached_test_function(&integer_choices(&[start]));
    ignore_run_is_complete(runner.optimise_targets());
    if score_up {
        assert_eq!(best(&runner, "n"), upper as f64);
    } else {
        assert_eq!(best(&runner, "n"), (-lower) as f64);
    }
}

#[test]
fn test_can_find_endpoints_of_a_range_0_1000_score_down() {
    check_find_endpoints(0, 1000, false);
}

#[test]
fn test_can_find_endpoints_of_a_range_0_1000_score_up() {
    check_find_endpoints(0, 1000, true);
}

#[test]
fn test_can_find_endpoints_of_a_range_13_100_score_down() {
    check_find_endpoints(13, 100, false);
}

#[test]
fn test_can_find_endpoints_of_a_range_13_100_score_up() {
    check_find_endpoints(13, 100, true);
}

#[test]
fn test_can_find_endpoints_of_a_range_1000_65535_score_down() {
    check_find_endpoints(1000, (1i128 << 16) - 1, false);
}

#[test]
fn test_can_find_endpoints_of_a_range_1000_65535_score_up() {
    check_find_endpoints(1000, (1i128 << 16) - 1, true);
}

#[test]
fn test_targeting_can_drive_length_very_high() {
    let mut runner = TargetedRunner::new(
        |data| {
            let mut count: i64 = 0;
            while data.draw_boolean(0.25) {
                count += 1;
            }
            observe(data, "", count.min(100) as f64);
        },
        runner_settings(),
        rng(),
    );
    // extend=50 ensures we get a valid (non-overrun) seed case. The
    // outcome doesn't matter; we just need something for the optimiser to
    // climb from.
    runner.cached_test_function_extend(&[], 50);
    ignore_run_is_complete(runner.optimise_targets());
    assert_eq!(best(&runner, ""), 100.0);
}

#[test]
fn test_optimiser_when_test_grows_buffer_to_invalid() {
    let mut runner = TargetedRunner::new(
        |data| {
            let m = data.draw_integer(0, (1i128 << 8) - 1);
            observe(data, "m", m as f64);
            if m > 100 {
                data.draw_integer(0, (1i128 << 16) - 1);
                data.mark_invalid();
            }
        },
        runner_settings(),
        rng(),
    );
    runner.cached_test_function(&integer_choices(&[0; 10]));
    ignore_run_is_complete(runner.optimise_targets());
    assert_eq!(best(&runner, "m"), 100.0);
}

#[test]
fn test_can_patch_up_examples() {
    let mut runner = TargetedRunner::new(
        |data| {
            data.start_span(42);
            let m = data.draw_integer(0, (1i128 << 6) - 1);
            observe(data, "m", m as f64);
            for _ in 0..m {
                data.draw_boolean(0.5);
            }
            data.stop_span();
            for i in 0..4i128 {
                if i != data.draw_integer(0, (1i128 << 8) - 1) {
                    data.mark_invalid();
                }
            }
        },
        runner_settings().max_examples(1000),
        rng(),
    );
    let d = runner.cached_test_function(&integer_choices(&[0, 0, 1, 2, 3, 4]));
    assert_eq!(d.status, Status::Valid);
    ignore_run_is_complete(runner.optimise_targets());
    assert_eq!(best(&runner, "m"), 63.0);
}

#[test]
fn test_optimiser_when_test_grows_buffer_to_overflow() {
    let mut runner = TargetedRunner::new(
        |data| {
            let m = data.draw_integer(0, (1i128 << 8) - 1);
            observe(data, "m", m as f64);
            if m > 100 {
                // Python uses 2**64-1; the i128 cast preserves the range.
                data.draw_integer(0, (1i128 << 64) - 1);
                data.mark_invalid();
            }
        },
        runner_settings(),
        rng(),
    );

    {
        let _guard = BufferSizeLimit::new(2);
        runner.cached_test_function(&integer_choices(&[0; 10]));
        ignore_run_is_complete(runner.optimise_targets());
    }

    assert_eq!(best(&runner, "m"), 100.0);
}

// The upstream `test_optimising_all_nodes` has three `@example` rows plus a
// `@given(nodes())` body. Both are ported below: the three fixed examples
// as independent `#[test]`s, and the `@given` branch as
// `test_optimising_all_nodes` driven by the `nodes()` composite generator.

fn run_optimise_all_nodes_bytes(initial: Vec<u8>, min_size: usize, max_size: usize) {
    let mut runner = TargetedRunner::new(
        move |data| {
            let v = data.draw_bytes(min_size, max_size);
            observe(data, "v", v.len() as f64);
        },
        runner_settings().max_examples(50),
        rng(),
    );
    runner.cached_test_function(&[ChoiceValue::Bytes(initial)]);
    ignore_run_is_complete(runner.optimise_targets());
}

fn run_optimise_all_nodes_string(
    initial: &str,
    intervals: IntervalSet,
    min_size: usize,
    max_size: usize,
) {
    let initial_cp: Vec<u32> = initial.chars().map(|c| c as u32).collect();
    let mut runner = TargetedRunner::new(
        move |data| {
            let v = data.draw_string(&intervals, min_size, max_size);
            observe(data, "v", v.chars().count() as f64);
        },
        runner_settings().max_examples(50),
        rng(),
    );
    runner.cached_test_function(&[ChoiceValue::String(initial_cp)]);
    ignore_run_is_complete(runner.optimise_targets());
}

fn run_optimise_all_nodes_integer(initial: i128, min_value: i128, max_value: i128) {
    let mut runner = TargetedRunner::new(
        move |data| {
            let v = data.draw_integer(min_value, max_value);
            observe(data, "v", v as f64);
        },
        runner_settings().max_examples(50),
        rng(),
    );
    runner.cached_test_function(&integer_choices(&[initial]));
    ignore_run_is_complete(runner.optimise_targets());
}

#[test]
fn test_optimising_all_nodes_bytes_example() {
    // @example(ChoiceNode(type="bytes", value=b"\xb1",
    //                      constraints={"min_size": 1, "max_size": 1}))
    run_optimise_all_nodes_bytes(vec![0xb1], 1, 1);
}

#[test]
fn test_optimising_all_nodes_string_example() {
    // @example(ChoiceNode(type="string", value="aaaa",
    //         constraints={"min_size": 0, "max_size": 10,
    //                      "intervals": IntervalSet.from_string("abcd")}))
    let intervals = IntervalSet::new(vec![('a' as u32, 'd' as u32)]);
    run_optimise_all_nodes_string("aaaa", intervals, 0, 10);
}

#[test]
fn test_optimising_all_nodes_integer_example() {
    // @example(ChoiceNode(type="integer", value=1,
    //                      constraints=integer_constr(0, 200)))
    run_optimise_all_nodes_integer(1, 0, 200);
}

// ── nodes() strategy ──────────────────────────────────────────────────────
//
// Port of upstream's `nodes()` strategy from
// `hypothesis-python/tests/conjecture/common.py`, adapted to hegel's
// concrete `ChoiceKind` representation. Each per-kind composite draws a
// valid `(ChoiceKind, ChoiceValue)` pair; `nodes()` picks one uniformly at
// random. The per-kind bounds mirror the `constraints_strategy` /
// `*_constraints` building blocks from
// `hypothesis/internal/conjecture/provider_conformance.py`, trimmed to
// ranges that keep `compute_max_children` workable without frequent overruns.

#[hegel::composite]
fn integer_node(tc: TestCase) -> (ChoiceKind, ChoiceValue) {
    let lo: i64 = tc.draw(
        gs::integers::<i64>()
            .min_value(-(1_i64 << 32))
            .max_value(1_i64 << 32),
    );
    let extra: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(1_i64 << 32));
    let kind = IntegerChoice {
        min_value: i128::from(lo),
        max_value: i128::from(lo + extra),
    };
    let value: i64 = tc.draw(gs::integers::<i64>().min_value(lo).max_value(lo + extra));
    (
        ChoiceKind::Integer(kind),
        ChoiceValue::Integer(i128::from(value)),
    )
}

#[hegel::composite]
fn boolean_node(tc: TestCase) -> (ChoiceKind, ChoiceValue) {
    let value: bool = tc.draw(gs::booleans());
    (
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
    )
}

#[hegel::composite]
fn float_node(tc: TestCase) -> (ChoiceKind, ChoiceValue) {
    let lo: f64 = tc.draw(gs::floats::<f64>().min_value(-1e9).max_value(1e9));
    let hi: f64 = tc.draw(gs::floats::<f64>().min_value(lo).max_value(lo + 1e9));
    let allow_nan: bool = tc.draw(gs::booleans());
    let allow_infinity: bool = tc.draw(gs::booleans());
    let kind = FloatChoice {
        min_value: lo,
        max_value: hi,
        allow_nan,
        allow_infinity,
    };
    let value: f64 = tc.draw(gs::floats::<f64>().min_value(lo).max_value(hi));
    (ChoiceKind::Float(kind), ChoiceValue::Float(value))
}

#[hegel::composite]
fn bytes_node(tc: TestCase) -> (ChoiceKind, ChoiceValue) {
    let min_size: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(4));
    let extra: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(8));
    let max_size = min_size + extra;
    let kind = BytesChoice { min_size, max_size };
    let length: usize = tc.draw(
        gs::integers::<usize>()
            .min_value(min_size)
            .max_value(max_size),
    );
    let value: Vec<u8> = tc.draw(gs::binary().min_size(length).max_size(length));
    (ChoiceKind::Bytes(kind), ChoiceValue::Bytes(value))
}

#[hegel::composite]
fn string_node(tc: TestCase) -> (ChoiceKind, ChoiceValue) {
    // Keep the codepoint range inside ASCII so `draw_string`'s
    // collapsed-interval approximation is faithful to the requested bounds.
    let min_cp: u32 = tc.draw(gs::integers::<u32>().min_value(32).max_value(100));
    let max_cp: u32 = tc.draw(
        gs::integers::<u32>()
            .min_value(min_cp)
            .max_value((min_cp + 30).min(126)),
    );
    let min_size: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(3));
    let extra: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(8));
    let max_size = min_size + extra;
    let kind = StringChoice {
        min_codepoint: min_cp,
        max_codepoint: max_cp,
        min_size,
        max_size,
    };
    let length: usize = tc.draw(
        gs::integers::<usize>()
            .min_value(min_size)
            .max_value(max_size),
    );
    let value: String = tc.draw(
        gs::text()
            .min_codepoint(min_cp)
            .max_codepoint(max_cp)
            .min_size(length)
            .max_size(length),
    );
    let cps: Vec<u32> = value.chars().map(|c| c as u32).collect();
    (ChoiceKind::String(kind), ChoiceValue::String(cps))
}

#[hegel::composite]
fn nodes(tc: TestCase) -> (ChoiceKind, ChoiceValue) {
    // Upstream's `choice_types_constraints` picks one of the five kinds
    // uniformly via `st.one_of`. Dispatch on a small integer index to match.
    let which: u8 = tc.draw(gs::integers::<u8>().min_value(0).max_value(4));
    match which {
        0 => tc.draw(integer_node()),
        1 => tc.draw(boolean_node()),
        2 => tc.draw(float_node()),
        3 => tc.draw(bytes_node()),
        _ => tc.draw(string_node()),
    }
}

fn float_size(f: f64) -> f64 {
    if f.is_finite() { f } else { 0.0 }
}

#[hegel::test(test_cases = 50)]
fn test_optimising_all_nodes(tc: TestCase) {
    let (kind, value) = tc.draw(nodes());
    tc.assume(compute_max_children(&kind) > BigUint::from(50u32));

    match kind {
        ChoiceKind::Integer(ic) => {
            let (lo, hi) = (ic.min_value, ic.max_value);
            let mut runner = TargetedRunner::new(
                move |data| {
                    let v = data.draw_integer(lo, hi);
                    observe(data, "v", v as f64);
                },
                runner_settings().max_examples(50),
                rng(),
            );
            runner.cached_test_function(&[value]);
            ignore_run_is_complete(runner.optimise_targets());
        }
        ChoiceKind::Boolean(_) => {
            let mut runner = TargetedRunner::new(
                move |data| {
                    let v = data.draw_boolean(0.5);
                    observe(data, "v", if v { 1.0 } else { 0.0 });
                },
                runner_settings().max_examples(50),
                rng(),
            );
            runner.cached_test_function(&[value]);
            ignore_run_is_complete(runner.optimise_targets());
        }
        ChoiceKind::Float(fc) => {
            let (lo, hi, nan, inf) = (fc.min_value, fc.max_value, fc.allow_nan, fc.allow_infinity);
            let mut runner = TargetedRunner::new(
                move |data| {
                    let v = data.draw_float(lo, hi, nan, inf);
                    observe(data, "v", float_size(v));
                },
                runner_settings().max_examples(50),
                rng(),
            );
            runner.cached_test_function(&[value]);
            ignore_run_is_complete(runner.optimise_targets());
        }
        ChoiceKind::Bytes(bc) => {
            let (min_size, max_size) = (bc.min_size, bc.max_size);
            let mut runner = TargetedRunner::new(
                move |data| {
                    let v = data.draw_bytes(min_size, max_size);
                    observe(data, "v", v.len() as f64);
                },
                runner_settings().max_examples(50),
                rng(),
            );
            runner.cached_test_function(&[value]);
            ignore_run_is_complete(runner.optimise_targets());
        }
        ChoiceKind::String(sc) => {
            let intervals = IntervalSet::new(vec![(sc.min_codepoint, sc.max_codepoint)]);
            let (min_size, max_size) = (sc.min_size, sc.max_size);
            let mut runner = TargetedRunner::new(
                move |data| {
                    let v = data.draw_string(&intervals, min_size, max_size);
                    observe(data, "v", v.chars().count() as f64);
                },
                runner_settings().max_examples(50),
                rng(),
            );
            runner.cached_test_function(&[value]);
            ignore_run_is_complete(runner.optimise_targets());
        }
    }
}
