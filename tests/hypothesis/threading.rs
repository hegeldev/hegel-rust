//! Ported from hypothesis-python/tests/cover/test_threading.py

use hegel::generators::{self as gs};
use hegel::{Hegel, Settings};
use std::sync::{Arc, Barrier};
use std::thread;

/// Omitted: test_threadlocal_setattr_and_getattr, test_nonexistent_getattr_raises,
/// test_nonexistent_setattr_raises, test_raises_if_not_passed_callable — these test
/// hypothesis.utils.threading.ThreadLocal, a Python-specific utility class wrapping
/// Python's threading.local() with dunder attribute access; no Rust counterpart.
///
/// Omitted: TestNoDifferingExecutorsHealthCheck — relies on pytest parametrize class
/// instantiation behavior; Python/pytest-specific infrastructure.
#[test]
fn test_run_given_concurrently() {
    let n_threads = 2;
    let barrier = Arc::new(Barrier::new(n_threads));

    let handles: Vec<_> = (0..n_threads)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                Hegel::new(move |tc| {
                    let _n: i64 = tc.draw(gs::integers());
                    barrier.wait();
                })
                .settings(Settings::new().database(None))
                .run();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
