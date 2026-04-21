// Ported from resources/pbtkit/tests/test_spans.py (span-mutation tests).
//
// These tests need direct access to the private `try_span_mutation` function,
// so they live here as an embedded submodule of runner.rs.

use crate::native::core::{ChoiceKind, ChoiceNode, ChoiceValue, IntegerChoice, NativeTestCase};
use crate::native::shrinker::ShrinkRun;
use crate::native::tree::CachedTestFunction;
use crate::test_case::TestCase;

#[test]
fn test_span_mutation_noop_without_spans() {
    // Span mutation hook does nothing when test case has no spans.
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    let mut ctf = CachedTestFunction::new(|_: TestCase| {});
    let choices = vec![ChoiceValue::Integer(0)];
    let ntc = NativeTestCase::for_choices(&choices, None);
    let (_, nodes, spans) = ctf.run(ntc);

    assert!(spans.is_empty());

    let mut rng = SmallRng::seed_from_u64(0);
    let result = super::try_span_mutation(&nodes, &spans, &mut rng, &mut ctf);
    assert!(result.is_none());
}

#[test]
fn test_span_mutation_exercises_swaps() {
    // Span mutation hook makes extra test-function calls when the test case
    // has multiple spans sharing the same label (e.g. two integer draws of
    // the same schema).
    use crate::generators::{self as gs};
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    for seed in 0u64..20 {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            let list_gen = gs::vecs(gs::tuples!(
                gs::integers::<i64>().min_value(0).max_value(3),
                gs::integers::<i64>().min_value(0).max_value(3),
            ))
            .min_size(2)
            .max_size(5);
            tc.draw(&list_gen);
        });

        let mut rng = SmallRng::seed_from_u64(seed);
        let batch_rng = SmallRng::from_rng(&mut rng);
        let ntc = NativeTestCase::new_random(batch_rng);
        let (_, nodes, spans) = ctf.run(ntc);
        let base_calls = calls.load(Ordering::SeqCst);

        let mut swap_rng = SmallRng::seed_from_u64(seed);
        super::try_span_mutation(&nodes, &spans, &mut swap_rng, &mut ctf);
        if calls.load(Ordering::SeqCst) > base_calls {
            return;
        }
    }
    panic!("no seed produced span mutation swaps");
}

#[test]
fn dispatch_shrink_run_full_routes_to_run_shrink() {
    use crate::generators as gs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let observed = Arc::new(AtomicUsize::new(0));
    let observed_clone = observed.clone();
    let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
        let v = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        observed_clone.store(v as usize, Ordering::SeqCst);
    });

    let kind = ChoiceKind::Integer(IntegerChoice {
        min_value: 0,
        max_value: 100,
    });
    let nodes = vec![ChoiceNode {
        kind,
        value: ChoiceValue::Integer(7),
        was_forced: false,
    }];

    let (_interesting, actual) = super::dispatch_shrink_run(ShrinkRun::Full(&nodes), &mut ctf);
    assert_eq!(observed.load(Ordering::SeqCst), 7);
    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].value, ChoiceValue::Integer(7));
}

#[test]
fn dispatch_shrink_run_probe_routes_to_run_probe() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
        calls_clone.fetch_add(1, Ordering::SeqCst);
        use crate::generators as gs;
        let _ = tc.draw(gs::booleans());
    });

    let prefix: Vec<ChoiceValue> = Vec::new();
    let (_interesting, _actual) = super::dispatch_shrink_run(
        ShrinkRun::Probe {
            prefix: &prefix,
            seed: 42,
            max_size: 4,
        },
        &mut ctf,
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
