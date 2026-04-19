//! Ported from hypothesis-python/tests/cover/test_feature_flags.py

#![cfg(feature = "native")]

use crate::common::utils::{assert_all_examples, find_any, minimal};
use hegel::__native_test_internals::{FeatureFlags, FeatureStrategy};

fn strat() -> FeatureStrategy {
    FeatureStrategy::new()
}

#[test]
fn test_can_all_be_enabled() {
    find_any(strat(), |x: &FeatureFlags| {
        (0..100).all(|i| x.is_enabled(&i.to_string()))
    });
}

#[test]
fn test_minimizes_open() {
    let features: Vec<String> = (0..10).map(|i| i.to_string()).collect();
    let features_for_cond = features.clone();
    let flags = minimal(strat(), move |x: &FeatureFlags| {
        for n in &features_for_cond {
            x.is_enabled(n);
        }
        true
    });
    for n in &features {
        assert!(flags.is_enabled(n));
    }
}

#[test]
fn test_minimizes_individual_features_to_open() {
    let features: Vec<String> = (0..10).map(|i| i.to_string()).collect();
    let features_for_cond = features.clone();
    let flags = minimal(strat(), move |x: &FeatureFlags| {
        let enabled: usize = features_for_cond
            .iter()
            .map(|n| x.is_enabled(n) as usize)
            .sum();
        enabled < features_for_cond.len()
    });
    for n in &features[..features.len() - 1] {
        assert!(flags.is_enabled(n));
    }
    assert!(!flags.is_enabled(&features[features.len() - 1]));
}

#[test]
fn test_marks_unknown_features_as_enabled() {
    let x = find_any(strat(), |_: &FeatureFlags| true);
    assert!(x.is_enabled("fish"));
}

#[test]
fn test_by_default_all_enabled() {
    let f = FeatureFlags::new();
    assert!(f.is_enabled("foo"));
}

// Omitted: test_eval_featureflags_repr and test_repr_can_be_evalled — both
// rely on Python's eval(repr(...)) round-trip, which has no Rust counterpart.

#[test]
fn test_can_avoid_disabling_every_flag() {
    let s = FeatureStrategy::new().at_least_one_of(["a", "b", "c"]);
    assert_all_examples(s, |flags: &FeatureFlags| {
        ["a", "b", "c"].iter().any(|k| flags.is_enabled(k))
    });
}
