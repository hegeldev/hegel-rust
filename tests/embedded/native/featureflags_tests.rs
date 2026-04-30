// Embedded tests for src/native/featureflags.rs — exercise the paths that
// the integration tests in tests/hypothesis/feature_flags.rs don't reach:
// pre-seeded with_flags, Default, Debug, the live/handle-missing fallback,
// and the StopTest panic branches in is_enabled / do_draw.

use std::collections::HashSet;
use std::sync::Arc;

use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::native::core::NativeTestCase;
use crate::native::data_source::NativeDataSource;
use crate::runner::Mode;
use crate::test_case::TestCase;

#[test]
fn with_flags_seeds_enabled_and_disabled() {
    let flags = FeatureFlags::with_flags(["alpha", "beta"], ["gamma"]);
    assert!(flags.is_enabled("alpha"));
    assert!(flags.is_enabled("beta"));
    assert!(!flags.is_enabled("gamma"));
    assert!(flags.is_enabled("delta"));
}

#[test]
fn default_is_all_enabled() {
    let flags = FeatureFlags::default();
    assert!(flags.is_enabled("anything"));
}

#[test]
fn debug_formats_sorted_enabled_and_disabled() {
    let flags = FeatureFlags::with_flags(["b", "a"], ["d", "c"]);
    let s = format!("{:?}", flags);
    assert!(s.contains("FeatureFlags"));
    let enabled_idx = s.find("enabled: ").unwrap();
    let disabled_idx = s.find("disabled: ").unwrap();
    let a_idx = s.find("\"a\"").unwrap();
    let b_idx = s.find("\"b\"").unwrap();
    let c_idx = s.find("\"c\"").unwrap();
    let d_idx = s.find("\"d\"").unwrap();
    assert!(enabled_idx < a_idx && a_idx < b_idx && b_idx < disabled_idx);
    assert!(disabled_idx < c_idx && c_idx < d_idx);
}

#[test]
fn live_flags_fall_back_to_enabled_when_handle_missing() {
    // A live FeatureFlags with no test-case handle (either the test
    // completed or the user constructed one manually) should default to
    // every feature enabled — matching Hypothesis's "data frozen" fallback.
    let flags = FeatureFlags::live(0.5, HashSet::new(), None);
    assert!(flags.is_enabled("unknown"));
}

#[test]
fn live_flags_remember_prior_decisions_when_handle_missing() {
    // Live flags, with the handle dropped. Prior decisions persist; anything
    // new defaults to enabled.
    let flags = {
        let ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(1));
        let (data_source, handle) = NativeDataSource::new(ntc);
        let strategy = FeatureStrategy::new();
        let mut tc = TestCase::new(Box::new(data_source), false, Mode::TestRun);
        tc.attach_native_handle(handle);
        let flags = strategy.do_draw(&tc);
        // Force a decision while the handle is still live.
        let _ = flags.is_enabled("recorded");
        flags
        // tc is dropped here: all strong Arc refs go away → Weak fails to upgrade
    };
    // Weak upgrade now fails, so is_enabled falls back to frozen-mode default.
    assert!(flags.is_enabled("brand_new"));
}

#[test]
fn at_least_one_of_single_name_forces_enabled() {
    // Exercises the `oneof.len() == 1 && oneof.contains(name)` branch in
    // is_enabled that forces `false` (i.e. "not disabled") for the sole
    // required feature, regardless of p_disabled.
    let ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(42));
    let (data_source, handle) = NativeDataSource::new(ntc);
    let strategy = FeatureStrategy::new().at_least_one_of(["only"]);
    let mut tc = TestCase::new(Box::new(data_source), false, Mode::TestRun);
    tc.attach_native_handle(handle);
    let flags = strategy.do_draw(&tc);
    assert!(flags.is_enabled("only"));
}

#[test]
#[should_panic(expected = "__HEGEL_STOP_TEST")]
fn is_enabled_panics_stop_test_when_test_case_exhausted() {
    // An empty-prefix NativeTestCase with no RNG has max_size == 0; the first
    // draw returns StopTest, which is_enabled converts to a STOP_TEST_STRING
    // panic (caught by the runner to mark the test invalid).
    let ntc = NativeTestCase::for_choices(&[], None, None);
    let (data_source, handle) = NativeDataSource::new(ntc);
    let weak = Arc::downgrade(&handle);
    // Keep both strong refs alive so the Weak upgrade succeeds.
    let flags = FeatureFlags::live(0.5, HashSet::new(), Some(weak));
    let _keeper = (data_source, handle);
    flags.is_enabled("x");
}

#[test]
#[should_panic(expected = "__HEGEL_STOP_TEST")]
fn do_draw_panics_stop_test_when_test_case_exhausted() {
    let ntc = NativeTestCase::for_choices(&[], None, None);
    let (data_source, handle) = NativeDataSource::new(ntc);
    let strategy = FeatureStrategy::new();
    let mut tc = TestCase::new(Box::new(data_source), false, Mode::TestRun);
    tc.attach_native_handle(handle);
    strategy.do_draw(&tc);
}

#[test]
#[should_panic(expected = "FeatureStrategy::do_draw called outside the native test context")]
fn do_draw_outside_native_context_panics() {
    // FeatureStrategy only works when tc.native_tc_handle() is Some.
    // Without attach_native_handle the handle is None and the expect fires.
    let ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    let (data_source, _handle) = NativeDataSource::new(ntc);
    let strategy = FeatureStrategy::new();
    let tc = TestCase::new(Box::new(data_source), false, Mode::TestRun);
    strategy.do_draw(&tc);
}
