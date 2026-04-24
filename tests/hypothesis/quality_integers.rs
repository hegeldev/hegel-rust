//! Ported from hypothesis-python/tests/quality/test_integers.py.
//!
//! Server-only: the native backend's integer nasty-value list seeds
//! `min_value` and `max_value` but not `min_value + 1` /
//! `max_value - 1`, so 1000 draws only hit two of the four boundary
//! values. Hypothesis's `GLOBAL_CONSTANTS` seeds powers of 10 plus
//! their ±1 neighbours (see `_constants_integers` in
//! `hypothesis/internal/conjecture/providers.py`), which is why the
//! Python side hits all four.

#![cfg(not(feature = "native"))]

use hegel::generators as gs;
use hegel::{Hegel, Settings};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[test]
fn test_biases_towards_boundary_values() {
    let trillion: i64 = 10i64.pow(12);
    let boundary_vals: HashSet<i64> =
        [-trillion, -trillion + 1, trillion - 1, trillion].into_iter().collect();
    let seen = Arc::new(Mutex::new(boundary_vals));
    let seen_clone = Arc::clone(&seen);

    Hegel::new(move |tc| {
        let n: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(-trillion)
                .max_value(trillion),
        );
        seen_clone.lock().unwrap().remove(&n);
    })
    .settings(Settings::new().test_cases(1000).database(None))
    .run();

    let remaining = seen.lock().unwrap();
    assert!(
        remaining.is_empty(),
        "Expected to see all boundary vals, but still have {remaining:?}"
    );
}
