//! Embedded tests for `src/native/test_runner.rs` helpers.  Cover the
//! health-check diagnostics (TooSlow and the flaky-replay message) that the
//! runner folds into a failing `TestRunResult` instead of panicking, so no
//! panic crosses the FFI boundary into libhegel.

use super::*;
use std::time::Duration;

#[test]
fn too_slow_check_reports_when_under_threshold_and_unsuppressed() {
    let msg = too_slow_check(
        /* valid_test_cases */ 1,
        /* total_test_time */ Duration::from_secs(60),
        /* threshold */ Duration::from_secs(30),
        /* suppressed */ false,
    );
    assert!(msg.is_some(), "expected too_slow_check to report a failure");
    assert!(msg.unwrap().contains("TooSlow"));
}

#[test]
fn too_slow_check_quiet_when_suppressed() {
    assert!(
        too_slow_check(
            /* valid_test_cases */ 1,
            /* total_test_time */ Duration::from_secs(60),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ true,
        )
        .is_none()
    );
}

#[test]
fn too_slow_check_quiet_when_under_threshold() {
    assert!(
        too_slow_check(
            /* valid_test_cases */ 1,
            /* total_test_time */ Duration::from_secs(1),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ false,
        )
        .is_none()
    );
}

#[test]
fn too_slow_check_quiet_when_enough_valid_cases() {
    // Once enough valid cases have run, the health check is no longer
    // applied even if total_test_time exceeds the threshold.
    assert!(
        too_slow_check(
            /* valid_test_cases */ 10_000,
            /* total_test_time */ Duration::from_secs(60),
            /* threshold */ Duration::from_secs(30),
            /* suppressed */ false,
        )
        .is_none()
    );
}

#[test]
fn flaky_diagnostic_mentions_flaky() {
    assert!(flaky_diagnostic().contains("Flaky test detected"));
}

#[test]
fn invalid_thresholds_match_hypothesis() {
    // Ported from Hypothesis's `_invalid_thresholds(r=0.01, c=0.99)`
    // (`engine.py`), which evaluates to `INVALID_THRESHOLD_BASE = 458` and
    // `INVALID_PER_VALID = 100`. Pin the port so an always-reject run gives up
    // after `458 + 1 = 459` cases, matching the core engine.
    assert_eq!(invalid_thresholds(0.01, 0.99), (458, 100));
}

// ── cached_run / span-mutation caching ──
//
// Span mutation proposes choice sequences whose paths are frequently
// already covered by generation. Pre-fix the native backend ran the test
// body for every proposal (`ctx.execute`), executing the test ~6× as often
// as Hypothesis, which routes mutations through `cached_test_function`.
// These tests pin the cache/tree short-circuits that close that gap.

use std::cell::Cell;
use std::rc::Rc;

use crate::native::core::ChoiceKind;
use crate::native::core::choices::BooleanChoice;
use crate::native::data_tree::{DataTreeNode, record_tree};
use crate::run_lifecycle::run_test_case;

/// Build an [`Engine`] whose `run_case` runs `test_fn` and counts how many
/// times the test body actually executed, then hand both to `body`.
fn with_counting_ctx<T, B>(mut test_fn: T, body: B)
where
    T: FnMut(crate::TestCase),
    B: FnOnce(&mut Engine<'_>, &Rc<Cell<usize>>),
{
    crate::run_lifecycle::init_panic_hook();
    let exec_count = Rc::new(Cell::new(0usize));
    let counter = exec_count.clone();
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        counter.set(counter.get() + 1);
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new().database(None);
    let mut ctx = Engine::new(&settings, None, &mut run_case);
    body(&mut ctx, &exec_count);
}

fn bool_node(value: bool) -> ChoiceNode {
    ChoiceNode::new(
        ChoiceKind::Boolean(BooleanChoice),
        ChoiceValue::Boolean(value),
        false,
    )
}

#[test]
fn cached_run_skips_execution_when_tree_knows_the_path() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
        },
        |ctx, count| {
            // The tree already records a one-boolean run that concluded
            // Valid; replaying that path (plus an unread trailing choice,
            // as a duplicated span would produce) must not run the body.
            record_tree(&mut ctx.tree_root, &[bool_node(false)], Status::Valid, &[]);

            let (run, executed) =
                ctx.cached_run(&[ChoiceValue::Boolean(false), ChoiceValue::Boolean(true)]);
            assert_eq!(run.status, Status::Valid);
            assert!(!executed);
            assert_eq!(count.get(), 0);
        },
    );
}

#[test]
fn cached_run_executes_novel_then_serves_repeat_from_cache() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
        },
        |ctx, count| {
            let choices = [ChoiceValue::Boolean(true)];

            let (first, executed_first) = ctx.cached_run(&choices);
            assert!(executed_first);
            assert_eq!(count.get(), 1);

            // A second identical replay is served without re-running.
            let (second, executed_second) = ctx.cached_run(&choices);
            assert!(!executed_second);
            assert_eq!(count.get(), 1);
            assert_eq!(first.status, second.status);
        },
    );
}

#[test]
fn cached_run_reexecutes_known_interesting_path_to_recover_payload() {
    // The tree can record that a path was Interesting but not the failure's
    // nodes/origin, so a cached_run on a tree-known Interesting path falls
    // through to a real execution to recover that payload.
    with_counting_ctx(
        |tc| {
            if tc.draw(crate::generators::booleans()) {
                panic!("boom");
            }
        },
        |ctx, count| {
            record_tree(
                &mut ctx.tree_root,
                &[bool_node(true)],
                Status::Interesting,
                &[],
            );

            let (run, executed) = ctx.cached_run(&[ChoiceValue::Boolean(true)]);
            assert_eq!(run.status, Status::Interesting);
            assert!(executed);
            assert_eq!(count.get(), 1);
            assert!(run.origin.is_some());
        },
    );
}

#[test]
fn span_mutation_does_not_re_execute_identical_proposals() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
        },
        |ctx, count| {
            // Two spans of the same label, one nested in the other. Every
            // span-mutation attempt then proposes the *same* duplicated
            // choice sequence, so only the first proposal runs the body and
            // the rest are served from the cache.
            let nodes = vec![
                bool_node(false),
                bool_node(true),
                bool_node(false),
                bool_node(true),
            ];
            let span = |start, end| Span {
                start,
                end,
                label: "L".to_string(),
                depth: 0,
                parent: None,
                discarded: false,
            };
            let spans = vec![span(0, 4), span(1, 3)];

            ctx.try_span_mutation(&nodes, &spans);

            assert_eq!(count.get(), 1);
            // The single executed probe was recorded: one call, one valid
            // example consumed from the budget, nothing interesting.
            assert_eq!(ctx.calls, 1);
            assert_eq!(ctx.valid_test_cases, 1);
            assert!(ctx.interesting.is_empty());
        },
    );
}

#[test]
fn span_mutation_returns_interesting_proposal() {
    with_counting_ctx(
        // Panics on a `false` draw, so the all-`false` mutated proposal is
        // Interesting.
        |tc| {
            if !tc.draw(crate::generators::booleans()) {
                panic!("boom on false");
            }
        },
        |ctx, count| {
            // Nested same-label spans → the deterministic proposal duplicates
            // the (false) prefix, and the body's single draw resolves to
            // `false` → Interesting on the first probe.
            let nodes = vec![
                bool_node(false),
                bool_node(false),
                bool_node(false),
                bool_node(false),
            ];
            let span = |start, end| Span {
                start,
                end,
                label: "L".to_string(),
                depth: 0,
                parent: None,
                discarded: false,
            };
            let spans = vec![span(0, 4), span(1, 3)];

            ctx.try_span_mutation(&nodes, &spans);

            assert_eq!(count.get(), 1);
            assert_eq!(ctx.calls, 1);
            // An Interesting probe is not a valid example; budget untouched.
            assert_eq!(ctx.valid_test_cases, 0);
            let origin = ctx
                .interesting
                .keys()
                .next()
                .expect("the first proposal should be Interesting");
            assert!(origin.contains("Panic"));
        },
    );
}

#[test]
fn span_mutation_stops_when_example_budget_is_full() {
    with_counting_ctx(
        |tc| {
            tc.draw(crate::generators::booleans());
        },
        |ctx, count| {
            let nodes = vec![
                bool_node(false),
                bool_node(true),
                bool_node(false),
                bool_node(true),
            ];
            let span = |start, end| Span {
                start,
                end,
                label: "L".to_string(),
                depth: 0,
                parent: None,
                discarded: false,
            };
            let spans = vec![span(0, 4), span(1, 3)];

            // Budget already full: no probe should run.
            ctx.valid_test_cases = 100;
            ctx.try_span_mutation(&nodes, &spans);

            assert_eq!(count.get(), 0);
            assert_eq!(ctx.calls, 0);
            assert_eq!(ctx.valid_test_cases, 100);
        },
    );
}

#[test]
fn create_rng_default_backend_is_prng() {
    let settings = Settings::new().seed(Some(123));
    assert!(matches!(create_rng(&settings, None), EngineRng::Prng(_)));
}

#[cfg(unix)]
#[test]
fn create_rng_urandom_backend_reads_urandom() {
    let settings = Settings::new().backend(crate::settings::Backend::Urandom);
    assert!(matches!(create_rng(&settings, None), EngineRng::Urandom(_)));
}

#[test]
fn run_main_with_urandom_backend_generates_and_passes() {
    // End-to-end: the urandom backend drives the full engine (every draw
    // reads /dev/urandom) for a passing test. Exercises the urandom fill
    // path through the biased samplers.
    crate::run_lifecycle::init_panic_hook();
    let mut test_fn = |tc: crate::TestCase| {
        let _: i32 = tc.draw(crate::generators::integers());
    };
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new()
        .test_cases(20)
        .database(None)
        .backend(crate::settings::Backend::Urandom);
    let result = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    assert!(result.passed);
}

#[test]
fn run_main_with_urandom_backend_finds_counterexample() {
    // A test that always panics must still surface a failure under the
    // urandom backend, going through generation, shrinking (deterministic
    // concrete-choice replay), and final replay.
    crate::run_lifecycle::init_panic_hook();
    let mut test_fn = |tc: crate::TestCase| {
        let _: i32 = tc.draw(crate::generators::integers());
        panic!("always fails");
    };
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new()
        .test_cases(20)
        .database(None)
        .backend(crate::settings::Backend::Urandom);
    let result = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    assert!(!result.passed);
    assert!(
        result.failures[0].panic_message.contains("always fails"),
        "{:?}",
        result.failures
    );
}

#[test]
fn slow_shrink_warning_mentions_shrinking() {
    let w = slow_shrink_warning();
    assert!(w.contains("Shrinking"), "{w}");
    assert!(w.contains("stopped"), "{w}");
}

#[test]
fn run_main_stops_shrinking_when_budget_is_exhausted() {
    // Drive `run_main` with a zero shrink budget so the wall-clock cutoff
    // fires deterministically instead of after five minutes. The run must
    // still surface the failure (with the best, un-shrunk example) rather
    // than hang, and the slow-shrink warning path is exercised.
    crate::run_lifecycle::init_panic_hook();
    let mut test_fn = |tc: crate::TestCase| {
        // A non-trivial counterexample so there is real shrinking work that
        // the zero budget cuts short.
        let v: Vec<i32> = tc.draw(crate::generators::vecs(crate::generators::integers()));
        assert!(v.is_empty(), "non-empty vec");
    };
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new()
        .test_cases(200)
        .database(None)
        .derandomize(true);
    let result = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::ZERO,
    );
    assert!(!result.passed, "the failure must still be reported");
    assert!(
        result.failures[0].panic_message.contains("non-empty vec"),
        "{:?}",
        result.failures
    );
}

#[test]
fn run_main_reports_too_slow_at_call_site() {
    // Drive `run_main` with a zero TooSlow threshold so the (otherwise
    // 30s-gated) call-site early-return fires deterministically — instead of
    // relying on a test happening to exceed 30s of generation under coverage
    // instrumentation. The body draws a value so each case is non-trivial.
    crate::run_lifecycle::init_panic_hook();
    let mut test_fn = |tc: crate::TestCase| {
        tc.draw(crate::generators::booleans());
    };
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, is_final: bool| {
        let _ = run_test_case(ds, &mut test_fn, is_final, Mode::TestRun, Verbosity::Normal);
    };
    let settings = Settings::new().test_cases(100).database(None);
    let result = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::ZERO,
        Duration::from_secs(300),
    );
    assert!(!result.passed);
    assert!(
        result.failures[0].panic_message.contains("TooSlow"),
        "{:?}",
        result.failures
    );
}

// ── TestCasesTooLarge (too_large_check) ──

#[test]
fn too_large_check_reports_when_over_threshold_and_unsuppressed() {
    let msg = too_large_check(
        /* valid */ 0, /* overrun */ 20, /* suppressed */ false,
    );
    assert!(msg.is_some());
    assert!(msg.unwrap().contains("TestCasesTooLarge"));
}

#[test]
fn too_large_check_quiet_when_suppressed() {
    assert!(too_large_check(0, 20, true).is_none());
}

#[test]
fn too_large_check_quiet_when_under_threshold() {
    assert!(too_large_check(0, 19, false).is_none());
}

#[test]
fn too_large_check_quiet_when_enough_valid_cases() {
    assert!(too_large_check(10, 100, false).is_none());
}

// ── LargeInitialTestCase (large_initial_check) ──

#[test]
fn large_initial_check_reports_on_overrun() {
    let msg = large_initial_check(true, Status::Invalid, 0, false);
    assert!(msg.unwrap().contains("LargeInitialTestCase"));
}

#[test]
fn large_initial_check_reports_on_large_valid_example() {
    // node_count * 2 > BUFFER_SIZE.
    let msg = large_initial_check(false, Status::Valid, BUFFER_SIZE, false);
    assert!(msg.unwrap().contains("LargeInitialTestCase"));
}

#[test]
fn large_initial_check_quiet_for_small_valid_example() {
    assert!(large_initial_check(false, Status::Valid, 1, false).is_none());
}

#[test]
fn large_initial_check_quiet_when_suppressed() {
    assert!(large_initial_check(true, Status::Invalid, 0, true).is_none());
}

#[test]
fn large_initial_check_quiet_for_interesting() {
    // A bug found at the simplest example is reported as a failure, not a
    // health-check failure.
    assert!(large_initial_check(false, Status::Interesting, BUFFER_SIZE, false).is_none());
}

// ── overrun vs invalid distinction ──

#[test]
fn genuine_overrun_is_early_stop_and_not_recorded_in_the_tree() {
    // A genuine choice-budget overrun must be `Status::EarlyStop`, not
    // `Status::Invalid`. `record_tree` only records a conclusion for
    // `status >= Invalid`, so mislabelling an overrun would pin the path into
    // the data tree as a permanent dead-end.
    with_counting_ctx(
        |tc| {
            // Two draws against a one-choice budget: the second overruns.
            tc.draw(crate::generators::booleans());
            tc.draw(crate::generators::booleans());
        },
        |ctx, _count| {
            let (run, _mismatch) = ctx.test_function(NativeTestCase::for_simplest(1));
            assert_eq!(run.status, Status::EarlyStop);

            // The overrun path is therefore not concluded in the tree: a later
            // walk of the same prefix must re-execute (returns `None`) rather
            // than serve a cached dead-end.
            let mut tree = DataTreeNode::default();
            record_tree(&mut tree, &run.nodes, run.status, &[]);
            let choices: Vec<ChoiceValue> = run.nodes.iter().map(|n| n.value.clone()).collect();
            assert_eq!(crate::native::data_tree::simulate(&tree, &choices), None);
        },
    );
}

// ── ReproduceRunner (failure-blob replay) ──

/// A `run_case` body that marks any integer `>= 1_000_000` interesting.
/// Used to provoke (and later replay) a failure.
fn mark_large_interesting(ds: &(dyn crate::backend::DataSource + Send + Sync)) {
    let schema = crate::cbor_utils::cbor_map! {
        "type" => "integer",
        "min_value" => 0_i64,
        "max_value" => 2_000_000_i64,
    };
    match ds.generate(&schema) {
        Ok(ciborium::Value::Integer(i)) => {
            let n: i128 = i.into();
            if n >= 1_000_000 {
                ds.mark_complete(&TestCaseResult::Interesting(Failure {
                    panic_message: "n >= 1_000_000".to_string(),
                    diagnostic: "n >= 1_000_000\n".to_string(),
                    origin: "n >= 1_000_000".to_string(),
                    reproduce_blob: None,
                }));
            } else {
                ds.mark_complete(&TestCaseResult::Valid);
            }
        }
        _ => ds.mark_complete(&TestCaseResult::Overrun),
    }
}

/// Run the failing property once and return the reproduce blob the engine
/// attached to the (shrunk) counterexample.
fn discover_reproduce_blob() -> String {
    let settings = Settings::new().test_cases(200).seed(Some(7)).database(None);
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, _is_final: bool| {
        mark_large_interesting(&*ds);
    };
    let result = run_main(
        &settings,
        None,
        &mut run_case,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );
    assert!(!result.passed, "property should have failed");
    result.failures[0]
        .reproduce_blob
        .clone()
        .expect("native failure should carry a reproduce blob")
}

#[test]
fn reproduce_runner_replays_the_counterexample() {
    let blob = discover_reproduce_blob();

    // Replaying the blob runs exactly the encoded example and re-surfaces
    // the failure, carrying the same blob back.
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let mut run_case = |ds: Box<dyn crate::backend::DataSource + Send + Sync>, _is_final: bool| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        mark_large_interesting(&*ds);
    };
    let runner = ReproduceRunner { blob: blob.clone() };
    let result = runner.run(&Settings::new(), None, &mut run_case);

    assert!(!result.passed);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(
        result.failures[0].reproduce_blob.as_deref(),
        Some(blob.as_str())
    );
    // A replay bypasses generation entirely: exactly one test case runs.
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a blob replay should not generate"
    );
}

#[test]
fn reproduce_runner_panics_on_an_undecodable_blob() {
    // An undecodable blob is invalid input — it panics rather than producing
    // a `TestRunResult` failure.
    let result = std::panic::catch_unwind(|| {
        let runner = ReproduceRunner {
            blob: "not-a-valid-blob".to_string(),
        };
        runner.run(&Settings::new(), None, &mut |ds, _is_final| {
            ds.mark_complete(&TestCaseResult::Valid);
        });
    });
    let payload = result.unwrap_err();
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("could not be decoded"),
        "unexpected panic message: {msg}"
    );
}

#[test]
fn reproduce_runner_reports_a_blob_that_no_longer_fails() {
    let blob = discover_reproduce_blob();

    // A "fixed" test body that never reports interesting: replaying a stale
    // blob must surface that rather than silently passing.
    let runner = ReproduceRunner { blob };
    let result = runner.run(&Settings::new(), None, &mut |ds, _is_final| {
        let schema = crate::cbor_utils::cbor_map! {
            "type" => "integer",
            "min_value" => 0_i64,
            "max_value" => 2_000_000_i64,
        };
        let _ = ds.generate(&schema);
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert!(!result.passed);
    assert!(
        result.failures[0]
            .diagnostic
            .contains("no longer reproduces"),
        "unexpected diagnostic: {}",
        result.failures[0].diagnostic
    );
    // Reported as its own failure, not framed as a health-check failure.
    assert_eq!(result.failures[0].origin, "reproduce_failure");
}

// ── database reuse semantics ──

#[test]
fn reuse_replay_extends_past_stored_prefix() {
    // Hypothesis replays stored entries with extend="full": when the test now
    // draws more choices than the stored prefix holds, the replay continues
    // with fresh random draws instead of overrunning. The stored `[true]` is
    // one boolean short of what the test reads; it must still reproduce.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));

    let result = std::panic::catch_unwind(|| {
        crate::Hegel::new(|tc: crate::TestCase| {
            let a = tc.draw(crate::generators::booleans());
            let _b = tc.draw(crate::generators::booleans());
            assert!(!a, "replayed bug");
        })
        .settings(
            crate::Settings::new()
                .database(Some(path.clone()))
                .phases([crate::Phase::Reuse])
                .verbosity(crate::Verbosity::Quiet),
        )
        .__database_key("k".to_string())
        .run();
    });
    assert!(
        result.is_err(),
        "stored prefix one draw short must still reproduce via random extension"
    );
}

#[test]
fn reuse_consults_secondary_corpus_when_primary_fails_to_reproduce() {
    use crate::native::bignum::BigInt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // The primary entry no longer fails; the still-failing example only
    // exists in the secondary (historical) corpus, which the reuse phase
    // samples when the primary corpus comes up short.
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(7))]),
    );
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    db.save(
        &secondary_key,
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(4242))]),
    );

    let result = std::panic::catch_unwind(|| {
        crate::Hegel::new(|tc: crate::TestCase| {
            let n: i64 = tc.draw(crate::generators::integers::<i64>());
            assert_ne!(n, 4242, "secondary bug");
        })
        .settings(
            crate::Settings::new()
                .database(Some(path.clone()))
                .phases([crate::Phase::Reuse])
                .test_cases(10)
                .verbosity(crate::Verbosity::Quiet),
        )
        .__database_key("k".to_string())
        .run();
    });
    assert!(
        result.is_err(),
        "the secondary corpus entry must be replayed when primary finds nothing"
    );
}

#[test]
fn reuse_randomly_samples_secondary_corpus_when_it_overflows_the_shortfall() {
    use crate::native::bignum::BigInt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // One primary entry that no longer reproduces, plus a secondary corpus
    // larger than the shortfall (desired_size 2 - 1 primary = 1).  That
    // drives the partial Fisher-Yates sampling path: more historical
    // entries exist than the reuse phase wants, so only a random subset is
    // replayed.  Every secondary entry reproduces the bug, so the run must
    // fail no matter which one the sample happens to keep.
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(7))]),
    );
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    for n in [4242, 4243, 4244, 4245] {
        db.save(
            &secondary_key,
            &serialize_choices(&[ChoiceValue::Integer(BigInt::from(n))]),
        );
    }

    let result = std::panic::catch_unwind(|| {
        crate::Hegel::new(|tc: crate::TestCase| {
            let n: i64 = tc.draw(crate::generators::integers::<i64>());
            assert!(n < 4242, "secondary bug");
        })
        .settings(
            crate::Settings::new()
                .database(Some(path.clone()))
                .phases([crate::Phase::Reuse])
                .test_cases(2)
                .verbosity(crate::Verbosity::Quiet),
        )
        .__database_key("k".to_string())
        .run();
    });
    assert!(
        result.is_err(),
        "a sampled secondary entry must still reproduce the bug"
    );
}

#[test]
fn shrink_phase_drains_stale_secondary_corpus_entries() {
    use crate::native::bignum::BigInt;
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    let secondary_key = crate::native::data_tree::sub_key(b"k", b"secondary");
    let stale = serialize_choices(&[ChoiceValue::Integer(BigInt::from(5))]);
    db.save(&secondary_key, &stale);

    // A failing run replays small secondary entries as shrink jump-starts
    // and deletes them either way (Hypothesis's clear_secondary_key) — the
    // secondary corpus must not grow without bound across runs.
    let result = std::panic::catch_unwind(|| {
        crate::Hegel::new(|tc: crate::TestCase| {
            let n: i64 = tc.draw(crate::generators::integers::<i64>());
            assert!(n < 1000, "big bug");
        })
        .settings(
            crate::Settings::new()
                .database(Some(path.clone()))
                .test_cases(200)
                .verbosity(crate::Verbosity::Quiet),
        )
        .__database_key("k".to_string())
        .run();
    });
    assert!(result.is_err(), "the run should find the n >= 1000 bug");
    assert!(
        !db.fetch(&secondary_key).contains(&stale),
        "the stale secondary entry must be drained"
    );
}

#[test]
fn should_generate_more_stops_ten_seconds_after_first_bug() {
    // Within the call-count window but past the 10-second wall-clock cutoff
    // (engine.py's first_bug_found_time): stop hunting for more origins.
    assert!(should_generate_more(
        false,
        20,
        Some(15),
        Some(15),
        true,
        true,
        Some(std::time::Duration::from_secs(9)),
    ));
    assert!(!should_generate_more(
        false,
        20,
        Some(15),
        Some(15),
        true,
        true,
        Some(std::time::Duration::from_secs(11)),
    ));
}

#[test]
fn reuse_stops_after_first_reproduced_bug_without_multiple_reporting() {
    use crate::native::bignum::BigInt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // Two stored entries that both still reproduce.
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(1111))]),
    );
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(2222))]),
    );

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    CALLS.store(0, Ordering::SeqCst);
    let result = std::panic::catch_unwind(|| {
        crate::Hegel::new(|tc: crate::TestCase| {
            CALLS.fetch_add(1, Ordering::SeqCst);
            let n: i64 = tc.draw(crate::generators::integers::<i64>());
            assert!(n < 1000, "stored bug");
        })
        .settings(
            crate::Settings::new()
                .database(Some(path.clone()))
                .phases([crate::Phase::Reuse])
                .report_multiple_failures(false)
                .verbosity(crate::Verbosity::Quiet),
        )
        .__database_key("k".to_string())
        .run();
    });
    assert!(result.is_err(), "the stored bug should be reported");
    // One reuse replay (the first entry reproduces, so the loop breaks)
    // plus the final is_final replay of the counterexample.
    assert!(
        CALLS.load(Ordering::SeqCst) <= 2,
        "expected reuse to stop after the first reproduced bug, ran {} cases",
        CALLS.load(Ordering::SeqCst)
    );
}

#[test]
fn reuse_found_bug_skips_generation_entirely() {
    use crate::native::bignum::BigInt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    db.save(
        b"k",
        &serialize_choices(&[ChoiceValue::Integer(BigInt::from(4242))]),
    );

    // Hypothesis skips generation when the database replay already
    // reproduced a failure ("we'd rather report that they're still failing
    // ASAP than take the time to look for new ones"). With reuse replays
    // now recorded like any other run, the bug-window heuristic alone would
    // let generation probe for several extra calls; the explicit skip must
    // keep the body-call count at exactly reuse + final replay.
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    CALLS.store(0, Ordering::SeqCst);
    let result = std::panic::catch_unwind(|| {
        crate::Hegel::new(|tc: crate::TestCase| {
            CALLS.fetch_add(1, Ordering::SeqCst);
            let n: i64 = tc.draw(crate::generators::integers::<i64>());
            assert_ne!(n, 4242, "stored bug");
        })
        .settings(
            crate::Settings::new()
                .database(Some(path.clone()))
                .test_cases(200)
                .verbosity(crate::Verbosity::Quiet),
        )
        .__database_key("k".to_string())
        .run();
    });
    assert!(result.is_err(), "the stored bug should be reported");
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        2,
        "expected exactly one reuse replay and one final replay"
    );
}

#[test]
fn should_generate_more_stops_without_bug_markers() {
    // Defensive arm: a non-empty interesting map with no bug-window markers
    // cannot arise from run_main any more (every interesting run passes
    // through record_run, which sets them), but the standalone function
    // must still answer sensibly.
    assert!(!should_generate_more(
        false, 5, None, None, true, true, None
    ));
}

#[test]
fn reuse_detects_nondeterministic_generator_across_replays() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let db = DirectoryTestCaseDatabase::new(&path);
    // Two stored entries, so the reuse phase replays twice.
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(true)]));
    db.save(b"k", &serialize_choices(&[ChoiceValue::Boolean(false)]));

    // The generator alternates the drawn kind per invocation: with reuse
    // replays now recorded into the choice tree, the second replay
    // contradicts the first and must surface the non-determinism
    // diagnostic instead of silently mispredicting.
    static FLIP: AtomicUsize = AtomicUsize::new(0);
    FLIP.store(0, Ordering::SeqCst);
    let result = std::panic::catch_unwind(|| {
        crate::Hegel::new(|tc: crate::TestCase| {
            if FLIP.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
                tc.draw(crate::generators::booleans());
            } else {
                let _: i64 = tc.draw(crate::generators::integers::<i64>());
            }
        })
        .settings(
            crate::Settings::new()
                .database(Some(path.clone()))
                .phases([crate::Phase::Reuse])
                .verbosity(crate::Verbosity::Quiet),
        )
        .__database_key("k".to_string())
        .run();
    });
    let err = result.expect_err("non-determinism must be reported");
    let msg = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(msg.contains("non-deterministic"), "got: {msg}");
}
