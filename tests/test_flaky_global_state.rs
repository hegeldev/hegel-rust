mod common;

use hegel::TestCase;
use hegel::generators as gs;
use std::sync::atomic::{AtomicI32, Ordering};

static GLOBAL_COUNTER: AtomicI32 = AtomicI32::new(0);

#[hegel::test(nondeterminism_strictness = hegel::NondeterminismStrictness::Error)]
#[should_panic(expected = "Your data generation is non-deterministic")]
fn test_flaky_global_state(tc: TestCase) {
    let _x = tc.draw(gs::integers::<i32>().min_value(GLOBAL_COUNTER.load(Ordering::SeqCst)));
    GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
}

static QUIET_COUNTER: AtomicI32 = AtomicI32::new(0);

#[hegel::test]
fn test_flaky_global_state_passes_under_the_quiet_default(tc: TestCase) {
    let _x = tc.draw(gs::integers::<i32>().min_value(QUIET_COUNTER.load(Ordering::SeqCst)));
    QUIET_COUNTER.fetch_add(1, Ordering::SeqCst);
}

static WARN_COUNTER: AtomicI32 = AtomicI32::new(0);

#[hegel::test(nondeterminism_strictness = hegel::NondeterminismStrictness::Warn)]
fn test_flaky_global_state_passes_under_warn(tc: TestCase) {
    let _x = tc.draw(gs::integers::<i32>().min_value(WARN_COUNTER.load(Ordering::SeqCst)));
    WARN_COUNTER.fetch_add(1, Ordering::SeqCst);
}
