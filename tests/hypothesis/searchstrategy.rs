//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_searchstrategy.py
//!
//! Tests that rely on Python-specific facilities are not ported (see SKIPPED.md
//! for the full list and rationale):
//!
//! - `test_or_errors_when_given_non_strategy` — Python `|` operator overloading.
//! - `test_just_strategy_uses_repr`, `test_can_map_nameless`,
//!   `test_can_flatmap_nameless` — Python `__repr__` and `functools.partial`.
//! - `test_flatmap_with_invalid_expand` — Python dynamic typing; Rust's
//!   `flat_map` requires its closure to return a generator at compile time.
//! - `test_use_of_global_random_is_deprecated_in_given`,
//!   `test_use_of_global_random_is_deprecated_in_interactive_draws` — Python
//!   global `random` module and `@checks_deprecated_behaviour`.
//! - `test_jsonable*`, `test_to_jsonable_handles_reference_cycles` — test
//!   `hypothesis.strategies._internal.utils.to_jsonable`, a Python-only
//!   observability helper with no hegel-rust counterpart.
//! - `test_deferred_strategy_draw` — `st.deferred()` has no hegel-rust analog;
//!   Rust's static types don't support forward-referenced recursive strategies.

use crate::common::utils::{assert_simple_property, expect_panic};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings};

#[test]
fn test_can_map() {
    assert_simple_property(gs::integers::<i64>().map(|_| "foo"), |v: &&str| *v == "foo");
}

#[test]
fn test_example_raises_unsatisfiable_when_too_filtered() {
    expect_panic(
        || {
            Hegel::new(|tc| {
                let _: i64 = tc.draw(gs::integers::<i64>().filter(|_: &i64| false));
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "(?i)(health.check|FailedHealthCheck|filter|unsatisfiable)",
    );
}

#[cfg(feature = "native")]
#[test]
fn test_just_strategy_does_not_draw() {
    use hegel::__native_test_internals::{CachedTestFunction, NativeTestCase};
    use hegel::TestCase;
    use std::sync::{Arc, Mutex};

    let seen = Arc::new(Mutex::new(None::<String>));
    let seen_clone = Arc::clone(&seen);
    let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
        let v: String = tc.draw(gs::just("hello".to_string()));
        *seen_clone.lock().unwrap() = Some(v);
    });
    let ntc = NativeTestCase::for_choices(&[], None, None);
    let (_, nodes, _) = ctf.run(ntc);

    assert_eq!(seen.lock().unwrap().as_deref(), Some("hello"));
    assert!(nodes.is_empty());
}

#[cfg(feature = "native")]
#[test]
fn test_none_strategy_does_not_draw() {
    use hegel::__native_test_internals::{CachedTestFunction, NativeTestCase};
    use hegel::TestCase;
    use std::sync::{Arc, Mutex};

    let seen = Arc::new(Mutex::new(false));
    let seen_clone = Arc::clone(&seen);
    let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
        tc.draw(gs::unit());
        *seen_clone.lock().unwrap() = true;
    });
    let ntc = NativeTestCase::for_choices(&[], None, None);
    let (_, nodes, _) = ctf.run(ntc);

    assert!(*seen.lock().unwrap());
    assert!(nodes.is_empty());
}
