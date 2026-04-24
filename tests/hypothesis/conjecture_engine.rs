//! Ported from hypothesis-python/tests/conjecture/test_engine.py.
//!
//! The shrink-quality subset of `test_engine.py` — the
//! `test_can_shrink_variable_draws` / `test_can_shrink_variable_string_draws`
//! / `test_variable_size_string_increasing` /
//! `test_mildly_complicated_strategies` cluster — ports unchanged
//! through the public `minimal(strategy, condition)` API and lives
//! in this file.
//!
//! The remaining ~80 tests assert on `ConjectureRunner` runtime
//! bookkeeping and are ported through the `NativeConjectureRunner`
//! wrapper in `hegel::__native_test_internals`. That wrapper's
//! per-attribute stubs live in `src/native/conjecture_runner.rs`;
//! each port-loop cycle that lands one of those native-gated tests
//! fills in the attribute it touches. See
//! `.claude/skills/porting-tests/SKILL.md` under "`test_engine.py`-shape"
//! for the port path.
//!
//! Individually-skipped tests:
//! - `test_recursion_error_is_not_flaky`: relies on CPython's
//!   `RecursionError` stack-depth tricks (Python-only, skipped
//!   upstream on PyPy and under coverage). No Rust analog.
//!
#![cfg(feature = "native")]

use crate::common::utils::{expect_panic, minimal};
use hegel::__native_test_internals::{
    ChoiceValue, ExitReason, HealthCheckLabel, NativeConjectureData, NativeConjectureRunner,
    NativeRunnerSettings, RunnerPhase, interesting_origin, run_to_nodes,
};
use hegel::TestCase;
use hegel::generators as gs;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::cell::RefCell;
use std::rc::Rc;

/// Label used by `test_variadic_draw`'s `start_span(SOME_LABEL)` calls.
/// Mirrors `tests/conjecture/common.py::SOME_LABEL` which is
/// `calc_label_from_name("some label")`.  The exact numeric value
/// doesn't matter to the assertions — the spans only need to share a
/// label.
const SOME_LABEL: u64 = 1;

// Port of the `@st.composite strategy(draw)` defined inline in
// `test_can_shrink_variable_draws`. Draws a variable `n ∈ [0, 15]` and
// then `n` integer choices. The outer `n` is a separate choice node,
// which is what makes the shrinker's "shrink the length, then each
// element" pattern non-trivial.
#[hegel::composite]
fn variable_int_list(tc: TestCase) -> Vec<i64> {
    let n: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(15));
    let mut result = Vec::with_capacity(n);
    for _ in 0..n {
        result.push(tc.draw(gs::integers::<i64>().min_value(0).max_value(255)));
    }
    result
}

fn check_can_shrink_variable_draws(n_large: i64) {
    let target = 128 * n_large;
    let ints = minimal(variable_int_list(), move |v: &Vec<i64>| {
        v.iter().copied().map(i128::from).sum::<i128>() >= i128::from(target)
    });
    // should look like [target % 255] + [255] * (len - 1)
    let expected_first = target % 255;
    assert_eq!(ints[0], expected_first, "ints = {ints:?}");
    for x in &ints[1..] {
        assert_eq!(*x, 255, "ints = {ints:?}");
    }
}

#[test]
fn test_can_shrink_variable_draws_1() {
    check_can_shrink_variable_draws(1);
}

#[test]
fn test_can_shrink_variable_draws_5() {
    check_can_shrink_variable_draws(5);
}

#[test]
fn test_can_shrink_variable_draws_8() {
    check_can_shrink_variable_draws(8);
}

#[test]
fn test_can_shrink_variable_draws_15() {
    check_can_shrink_variable_draws(15);
}

// Port of the `@st.composite` that draws `n` first, then a string of
// exactly `n` ASCII characters.
#[hegel::composite]
fn variable_ascii_string(tc: TestCase) -> String {
    let n: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(20));
    tc.draw(gs::text().codec("ascii").min_size(n).max_size(n))
}

#[test]
fn test_can_shrink_variable_string_draws() {
    let s = minimal(variable_ascii_string(), |s: &String| {
        s.len() >= 10 && s.contains('a')
    });
    // Upstream TODO_BETTER_SHRINK: ideally `"0" * 9 + "a"`. In practice
    // the shrinker settles on a string matching `0+a`. We mirror that
    // weaker assertion so the test stays faithful to the upstream
    // expectation.
    let matches = s.bytes().all(|b| b == b'0' || b == b'a')
        && s.ends_with('a')
        && s.chars().filter(|c| *c == 'a').count() == 1
        && s.len() >= 2;
    assert!(matches, "s = {s:?}");
}

// Port of the `@st.composite` that inverts `n` so the *strategy's*
// `n` axis shrinks towards 10 rather than 0.
#[hegel::composite]
fn inverted_ascii_string(tc: TestCase) -> String {
    let n_drawn: usize = tc.draw(gs::integers::<usize>().min_value(0).max_value(10));
    let n = 10 - n_drawn;
    tc.draw(gs::text().codec("ascii").min_size(n).max_size(n))
}

#[test]
fn test_variable_size_string_increasing() {
    let s = minimal(inverted_ascii_string(), |s: &String| {
        s.len() >= 5 && s.contains('a')
    });
    // Same TODO_BETTER_SHRINK caveat as
    // `test_can_shrink_variable_string_draws`.
    let matches = s.bytes().all(|b| b == b'0' || b == b'a')
        && s.ends_with('a')
        && s.chars().filter(|c| *c == 'a').count() == 1
        && s.len() >= 2;
    assert!(matches, "s = {s:?}");
}

// Coverage tests for engine.py / shrinker.py code paths that are
// exercised by shrinking any mildly-complicated strategy. Upstream is
// a single parametrised test with three rows; the third
// (`st.sampled_from(enum.Flag(...))` → `bit_count(f.value) > 1`) uses
// Python's `enum.Flag` factory which has no direct Rust analogue — it
// builds a 64-bit flag type at runtime. The two `st.lists(...)` rows
// port directly.
#[test]
fn test_mildly_complicated_strategies_integers_list() {
    minimal(
        gs::vecs(gs::integers::<i64>()).min_size(5),
        |_: &Vec<i64>| true,
    );
}

#[test]
fn test_mildly_complicated_strategies_unique_text_list() {
    minimal(
        gs::vecs(gs::text()).min_size(2).unique(true),
        |_: &Vec<String>| true,
    );
}

// -----------------------------------------------------------------------
// `run_to_nodes` cluster.  Each of the tests below ports a
// `test_engine.py` test that decorates its body with
// `@run_to_nodes` and inspects the shrunk `data.nodes` — we go
// through the `hegel::__native_test_internals::run_to_nodes` free
// function that wraps a `NativeConjectureRunner`.
// -----------------------------------------------------------------------

fn node_bytes(v: &ChoiceValue) -> &[u8] {
    match v {
        ChoiceValue::Bytes(b) => b,
        other => panic!("expected Bytes, got {other:?}"),
    }
}

fn node_integer(v: &ChoiceValue) -> i128 {
    match v {
        ChoiceValue::Integer(n) => *n,
        other => panic!("expected Integer, got {other:?}"),
    }
}

#[test]
fn test_non_cloneable_intervals() {
    let nodes = run_to_nodes(|data: &mut NativeConjectureData| {
        data.draw_bytes(10, 10);
        data.draw_bytes(9, 9);
        data.mark_interesting(interesting_origin(None));
    });
    assert_eq!(nodes.len(), 2);
    assert_eq!(node_bytes(&nodes[0].value), vec![0u8; 10]);
    assert_eq!(node_bytes(&nodes[1].value), vec![0u8; 9]);
}

#[test]
fn test_deletable_draws() {
    let nodes = run_to_nodes(|data: &mut NativeConjectureData| {
        loop {
            let x = data.draw_bytes(2, 2);
            if x[0] == 255 {
                data.mark_interesting(interesting_origin(None));
            }
        }
    });
    assert_eq!(nodes.len(), 1);
    assert_eq!(node_bytes(&nodes[0].value), vec![0xff, 0x00]);
}

#[test]
fn test_variadic_draw() {
    let nodes = run_to_nodes(|data: &mut NativeConjectureData| {
        let mut all_nonzero_found = false;
        loop {
            data.start_span(SOME_LABEL);
            let n = data.draw_integer(0, 2) as usize;
            let drawn = if n > 0 {
                Some(data.draw_bytes(n, n))
            } else {
                None
            };
            data.stop_span();
            if let Some(bytes) = drawn {
                if !bytes.is_empty() && bytes.iter().all(|&b| b != 0) {
                    all_nonzero_found = true;
                }
            }
            if n == 0 {
                break;
            }
        }
        if all_nonzero_found {
            data.mark_interesting(interesting_origin(None));
        }
    });
    assert_eq!(nodes.len(), 3);
    assert_eq!(node_integer(&nodes[0].value), 1);
    assert_eq!(node_bytes(&nodes[1].value), vec![0x01]);
    assert_eq!(node_integer(&nodes[2].value), 0);
}

#[test]
fn test_draw_to_overrun() {
    let nodes = run_to_nodes(|data: &mut NativeConjectureData| {
        let first = data.draw_bytes(1, 1);
        let d = (first[0].wrapping_sub(8)) as usize;
        data.draw_bytes(128 * d, 128 * d);
        if d >= 2 {
            data.mark_interesting(interesting_origin(None));
        }
    });
    assert_eq!(nodes.len(), 2);
    assert_eq!(node_bytes(&nodes[0].value), vec![10u8]);
    assert_eq!(node_bytes(&nodes[1].value), vec![0u8; 128 * 2]);
}

#[test]
fn test_erratic_draws() {
    // Mirrors `with pytest.raises(FlakyStrategyDefinition)`: the data
    // generation produces a different schema on every call (different
    // `min_size`/`max_size` on `draw_bytes`), so the runner's
    // non-determinism check fires during generation.
    let n = Rc::new(RefCell::new(0usize));
    let n_clone = n.clone();
    expect_panic(
        std::panic::AssertUnwindSafe(move || {
            run_to_nodes(move |data: &mut NativeConjectureData| {
                let current = *n_clone.borrow();
                data.draw_bytes(current, current);
                let second = 255usize.saturating_sub(current);
                data.draw_bytes(second, second);
                if current == 255 {
                    data.mark_interesting(interesting_origin(None));
                } else {
                    *n_clone.borrow_mut() += 1;
                }
            });
        }),
        "non-deterministic",
    );
}

#[test]
fn test_no_read_no_shrink() {
    let count = Rc::new(RefCell::new(0u32));
    let count_clone = count.clone();
    let nodes = run_to_nodes(move |data: &mut NativeConjectureData| {
        *count_clone.borrow_mut() += 1;
        data.mark_interesting(interesting_origin(None));
    });
    assert!(nodes.is_empty());
    assert_eq!(*count.borrow(), 1);
}

#[test]
fn test_one_dead_branch() {
    let seen: Rc<RefCell<std::collections::HashSet<u8>>> =
        Rc::new(RefCell::new(std::collections::HashSet::new()));
    let seen_clone = seen.clone();
    run_to_nodes(move |data: &mut NativeConjectureData| {
        let i = data.draw_bytes(1, 1)[0];
        if i > 0 {
            data.mark_invalid();
        }
        let j = data.draw_bytes(1, 1)[0];
        let mut seen_ref = seen_clone.borrow_mut();
        if seen_ref.len() < 255 {
            seen_ref.insert(j);
        } else if !seen_ref.contains(&j) {
            drop(seen_ref);
            data.mark_interesting(interesting_origin(None));
        }
    });
}

#[test]
fn test_returns_forced() {
    let value: Vec<u8> = vec![0, 1, 2, 3];
    let value_clone = value.clone();
    let nodes = run_to_nodes(move |data: &mut NativeConjectureData| {
        data.draw_bytes_forced(value_clone.len(), value_clone.len(), value_clone.clone());
        data.mark_interesting(interesting_origin(None));
    });
    assert_eq!(nodes.len(), 1);
    assert_eq!(node_bytes(&nodes[0].value), value.as_slice());
}

#[test]
fn test_run_nothing() {
    // `phases=()` disables generation, reuse, and shrink.  The runner
    // must exit without ever calling the test function.
    let rng = SmallRng::seed_from_u64(0);
    let settings = NativeRunnerSettings::new().phases(Vec::new());
    let mut runner = NativeConjectureRunner::new(
        |_: &mut NativeConjectureData| {
            panic!("AssertionError");
        },
        settings,
        rng,
    );
    runner.run();
    assert_eq!(runner.call_count, 0);
}

#[test]
fn test_stops_after_max_examples_when_generating() {
    // `max_examples=1` and no interesting mark: runner must run the
    // test function exactly once before stopping on the valid-example
    // budget.
    let seen: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_clone = seen.clone();
    let rng = SmallRng::seed_from_u64(0);
    let settings = NativeRunnerSettings::new().max_examples(1);
    let mut runner = NativeConjectureRunner::new(
        move |data: &mut NativeConjectureData| {
            let bytes = data.draw_bytes(1, 1);
            seen_clone.borrow_mut().push(bytes);
        },
        settings,
        rng,
    );
    runner.run();
    assert_eq!(seen.borrow().len(), 1);
}

#[test]
fn test_phases_can_disable_shrinking() {
    // `phases=(reuse, generate)` omits `Shrink`.  The test function
    // marks interesting on its first call, so with shrinking disabled
    // only that single call is made and `seen` collects exactly one
    // 32-byte value.
    let seen: Rc<RefCell<std::collections::HashSet<Vec<u8>>>> =
        Rc::new(RefCell::new(std::collections::HashSet::new()));
    let seen_clone = seen.clone();
    let rng = SmallRng::seed_from_u64(0);
    let settings =
        NativeRunnerSettings::new().phases(vec![RunnerPhase::Reuse, RunnerPhase::Generate]);
    let mut runner = NativeConjectureRunner::new(
        move |data: &mut NativeConjectureData| {
            let bytes = data.draw_bytes(32, 32);
            seen_clone.borrow_mut().insert(bytes);
            data.mark_interesting(interesting_origin(None));
        },
        settings,
        rng,
    );
    runner.run();
    assert_eq!(seen.borrow().len(), 1);
}

#[test]
fn test_interleaving_engines() {
    // The outer test's `g` callback calls `outer.mark_interesting(...)`
    // from inside a nested `NativeConjectureRunner`.  The only way to
    // reach the outer data from a `'static` inner closure is via a
    // raw pointer captured by value; the outer data is guaranteed to
    // outlive every call to `g` because the inner runner runs entirely
    // inside the outer test function's body.
    let children_interesting: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let children_ref = children_interesting.clone();

    let nodes = run_to_nodes(move |data: &mut NativeConjectureData| {
        let seed_bytes = data.draw_bytes(1, 1);
        let mut seed = 0u64;
        for &b in &seed_bytes {
            seed = seed.wrapping_mul(256).wrapping_add(u64::from(b));
        }

        let outer_ptr: *mut NativeConjectureData = data as *mut _;
        let g = move |d2: &mut NativeConjectureData| {
            d2.draw_bytes(1, 1);
            // Safety: `outer_ptr` points to the outer
            // `NativeConjectureData`, which is live for the entire
            // execution of the outer test function.  The inner
            // runner's `run()` is called from within that function,
            // so every invocation of `g` happens while the outer
            // data is still on the stack above us.
            unsafe { (*outer_ptr).mark_interesting(interesting_origin(None)) };
        };

        let rng = SmallRng::seed_from_u64(seed);
        let mut runner = NativeConjectureRunner::new(g, NativeRunnerSettings::new(), rng);
        runner.run();
        let had_interesting = !runner.interesting_examples.is_empty();
        children_ref.borrow_mut().push(had_interesting);

        if had_interesting {
            data.mark_interesting(interesting_origin(None));
        }
    });
    assert_eq!(nodes.len(), 1);
    assert_eq!(node_bytes(&nodes[0].value), vec![0u8]);
    assert!(children_interesting.borrow().iter().all(|b| !b));
}

// Mirrors engine.py's `MIN_TEST_CALLS`; duplicated here so the assertion
// reads like the Python original.
const MIN_TEST_CALLS: usize = 10;
// Mirrors engine.py's `_invalid_thresholds(r=0.01, c=0.99)` output.
const INVALID_THRESHOLD_BASE: usize = 458;
const INVALID_PER_VALID: usize = 100;

#[test]
fn test_detects_flakiness() {
    // tf raises interesting on its first call, then never again; the
    // shrink phase re-plays the stored interesting example and finds
    // it no longer reproduces, exiting with Flaky.  The generation
    // phase keeps running after the first bug up to MIN_TEST_CALLS,
    // so the user's tf is called exactly `MIN_TEST_CALLS + 1` times
    // total (10 generation calls + 1 shrink-phase re-validation).
    let count = Rc::new(RefCell::new(0usize));
    let count_clone = count.clone();
    let failed_once = Rc::new(RefCell::new(false));
    let failed_once_clone = failed_once.clone();
    let rng = SmallRng::seed_from_u64(0);
    let mut runner = NativeConjectureRunner::new(
        move |data: &mut NativeConjectureData| {
            data.draw_bytes(1, 1);
            *count_clone.borrow_mut() += 1;
            let mut fo = failed_once_clone.borrow_mut();
            if !*fo {
                *fo = true;
                data.mark_interesting(interesting_origin(None));
            }
        },
        NativeRunnerSettings::new(),
        rng,
    );
    runner.run();
    assert_eq!(runner.exit_reason, Some(ExitReason::Flaky));
    assert_eq!(*count.borrow(), MIN_TEST_CALLS + 1);
}

#[test]
fn test_exit_because_max_iterations() {
    // tf marks invalid on every call; with `max_examples=1` the runner
    // must exit with MaxIterations once the invalid-call budget
    // (INVALID_THRESHOLD_BASE) trips, not spin for 10 * max_examples
    // iterations.
    let rng = SmallRng::seed_from_u64(0);
    let settings = NativeRunnerSettings::new()
        .max_examples(1)
        .suppress_health_check(vec![
            HealthCheckLabel::FilterTooMuch,
            HealthCheckLabel::TooSlow,
            HealthCheckLabel::LargeBaseExample,
            HealthCheckLabel::DataTooLarge,
        ]);
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.draw_integer(0, (1_i128 << 64) - 1);
            data.mark_invalid();
        },
        settings,
        rng,
    );
    runner.run();
    assert!(runner.call_count <= 1000, "call_count = {}", runner.call_count);
    assert_eq!(runner.exit_reason, Some(ExitReason::MaxIterations));
}

#[test]
fn test_max_iterations_with_all_invalid() {
    // assume(False) on every example: stop after INVALID_THRESHOLD_BASE + 1
    // invalid attempts.  The `>` (strict) check means the threshold is
    // tripped on call INVALID_THRESHOLD_BASE + 1, so call_count lands
    // on exactly that number.
    let rng = SmallRng::seed_from_u64(0);
    let settings = NativeRunnerSettings::new()
        .max_examples(10_000)
        .suppress_health_check(vec![
            HealthCheckLabel::FilterTooMuch,
            HealthCheckLabel::TooSlow,
            HealthCheckLabel::LargeBaseExample,
            HealthCheckLabel::DataTooLarge,
        ]);
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            data.draw_integer(0, (1_i128 << 64) - 1);
            data.mark_invalid();
        },
        settings,
        rng,
    );
    runner.run();
    assert_eq!(runner.call_count, INVALID_THRESHOLD_BASE + 1);
    assert_eq!(runner.exit_reason, Some(ExitReason::MaxIterations));
}

fn check_max_iterations_with_some_valid(n_valid: usize) {
    let valid_count = Rc::new(RefCell::new(0usize));
    let valid_count_clone = valid_count.clone();
    let rng = SmallRng::seed_from_u64(0);
    let settings = NativeRunnerSettings::new()
        .max_examples(10_000)
        .suppress_health_check(vec![
            HealthCheckLabel::FilterTooMuch,
            HealthCheckLabel::TooSlow,
            HealthCheckLabel::LargeBaseExample,
            HealthCheckLabel::DataTooLarge,
        ]);
    let mut runner = NativeConjectureRunner::new(
        move |data: &mut NativeConjectureData| {
            data.draw_integer(0, (1_i128 << 64) - 1);
            let mut vc = valid_count_clone.borrow_mut();
            if *vc < n_valid {
                *vc += 1;
            } else {
                drop(vc);
                data.mark_invalid();
            }
        },
        settings,
        rng,
    );
    runner.run();
    let expected = n_valid + INVALID_THRESHOLD_BASE + n_valid * INVALID_PER_VALID + 1;
    assert_eq!(runner.call_count, expected);
    assert_eq!(runner.exit_reason, Some(ExitReason::MaxIterations));
}

#[test]
fn test_max_iterations_with_some_valid_1() {
    check_max_iterations_with_some_valid(1);
}

#[test]
fn test_max_iterations_with_some_valid_2() {
    check_max_iterations_with_some_valid(2);
}

#[test]
fn test_max_iterations_with_some_valid_5() {
    check_max_iterations_with_some_valid(5);
}

#[test]
fn test_exit_because_shrink_phase_timeout() {
    // Python monkey-patches `time.perf_counter` to advance by 1000
    // seconds on every call; the native port injects the same clock
    // via `with_time_source`.  The shrink phase's deadline fires on
    // the first re-validation call, so the runner exits with
    // VerySlowShrinking and records the matching statistics entry.
    let val = Rc::new(RefCell::new(0i64));
    let val_clone = val.clone();
    let rng = SmallRng::seed_from_u64(0);
    let settings = NativeRunnerSettings::new().max_examples(100_000);
    let mut runner = NativeConjectureRunner::new(
        |data: &mut NativeConjectureData| {
            if data.draw_integer(0, (1_i128 << 64) - 1) > (1_i128 << 33) {
                data.mark_interesting(interesting_origin(None));
            }
        },
        settings,
        rng,
    )
    .with_time_source(move || {
        let mut v = val_clone.borrow_mut();
        *v += 1000;
        *v as f64
    });
    runner.run();
    assert_eq!(runner.exit_reason, Some(ExitReason::VerySlowShrinking));
    assert_eq!(
        runner.statistics.get("stopped-because").map(String::as_str),
        Some("shrinking was very slow"),
    );
}
