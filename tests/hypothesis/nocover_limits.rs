//! Ported from hypothesis-python/tests/nocover/test_limits.py.
//!
//! The upstream test uses `@given(st.random_module(), st.integers())`;
//! `st.random_module()` has no hegel-rust counterpart (it seeds Python's
//! global PRNG — see the `test_random_module.py` entry in SKIPPED.md),
//! so the port drops that argument. The property under test is that
//! `max_examples` / `test_cases` is respected exactly, which only needs
//! one draw.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase};

#[test]
fn test_max_examples_are_respected() {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&counter);
    Hegel::new(move |tc: TestCase| {
        tc.draw(gs::integers::<i64>());
        c.fetch_add(1, Ordering::Relaxed);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
    assert_eq!(counter.load(Ordering::Relaxed), 100);
}
