//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_filtered_strategy.py
//!
//! Individually-skipped tests:
//!
//! - `test_filtered_branches_are_all_filtered`,
//!   `test_filter_conditions_may_be_empty`,
//!   `test_nested_filteredstrategy_flattens_conditions` — all three construct
//!   Hypothesis's internal `FilteredStrategy` class directly and inspect its
//!   `.branches` / `.flat_conditions` / `.filtered_strategy` attributes.
//!   hegel-rust models filtering as a `Filtered<T, F, G>` wrapper generator
//!   with a single predicate: nested `.filter(...)` calls compose as nested
//!   wrappers rather than flattening, there is no `branches` on generators,
//!   and a predicate-less `Filtered` is not expressible. See SKIPPED.md.

#[cfg(feature = "native")]
#[test]
fn test_filter_iterations_are_marked_as_discarded() {
    use hegel::__native_test_internals::{
        CachedTestFunction, ChoiceValue, NativeTestCase, with_native_tc,
    };
    use hegel::TestCase;
    use hegel::generators::{self as gs, Generator};
    use std::sync::{Arc, Mutex};

    let drawn = Arc::new(Mutex::new(None::<i64>));
    let has_discards_seen = Arc::new(Mutex::new(false));
    let drawn_clone = Arc::clone(&drawn);
    let hd_clone = Arc::clone(&has_discards_seen);

    let mut ctf = CachedTestFunction::new(move |tc: TestCase| {
        let v: i64 = tc.draw(gs::integers::<i64>().filter(|x: &i64| *x == 0));
        *drawn_clone.lock().unwrap() = Some(v);
        let hd = with_native_tc(|handle| {
            handle
                .expect("native test case handle should be set during test execution")
                .lock()
                .unwrap()
                .has_discards
        });
        *hd_clone.lock().unwrap() = hd;
    });

    let ntc =
        NativeTestCase::for_choices(&[ChoiceValue::Integer(1), ChoiceValue::Integer(0)], None);
    ctf.run(ntc);

    assert_eq!(*drawn.lock().unwrap(), Some(0));
    assert!(*has_discards_seen.lock().unwrap());
}
