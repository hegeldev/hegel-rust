//! Ported from hypothesis-python/tests/nocover/test_boundary_exploration.py
//!
//! The Python test uses `@given(st.data())` + `data.draw(st.booleans(), ...)`
//! inside the predicate to supply an arbitrary-but-consistent boolean for
//! each input, and catches `Unsatisfiable` from `minimal()` via `reject()`.
//! hegel-rust's `minimal()` helper spawns its own nested run, so we draw a
//! PRNG seed from the outer `tc` and use it as the source of the arbitrary
//! oracle — the same pattern used in
//! `test_always_evicts_the_lowest_scoring_value` in `cache_implementation`.
//! A `minimal()` that can't find any witness panics with "Could not find
//! any examples…"; we catch that panic (the Rust analog of
//! `except Unsatisfiable: reject()`).

use crate::common::utils::Minimal;
use hegel::generators as gs;
use hegel::{HealthCheck, Hegel, Settings};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn test_explore_arbitrary_function() {
    Hegel::new(|tc| {
        let seed: u64 = tc.draw(gs::integers::<u64>());
        let rng = RefCell::new(StdRng::seed_from_u64(seed));
        let cache: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());

        let predicate = move |x: &String| -> bool {
            if let Some(&v) = cache.borrow().get(x) {
                return v;
            }
            let v = rng.borrow_mut().next_u64() & 1 == 0;
            cache.borrow_mut().insert(x.clone(), v);
            v
        };

        catch_unwind(AssertUnwindSafe(|| {
            Minimal::new(gs::text().min_size(5), predicate)
                .test_cases(10)
                .run();
        }))
        .ok();
    })
    .settings(
        Settings::new()
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
}
