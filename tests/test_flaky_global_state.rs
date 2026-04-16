// Non-determinism detection is a server-side feature; skip in native mode.
#![cfg(not(feature = "native"))]

mod common;

use hegel::TestCase;
use hegel::generators as gs;
use std::sync::atomic::{AtomicI32, Ordering};

static GLOBAL_COUNTER: AtomicI32 = AtomicI32::new(0);

#[hegel::test]
#[should_panic(expected = "Your data generation is non-deterministic")]
fn test_flaky_global_state(tc: TestCase) {
    let _x = tc.draw(gs::integers::<i32>().min_value(GLOBAL_COUNTER.load(Ordering::SeqCst)));
    GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
}
