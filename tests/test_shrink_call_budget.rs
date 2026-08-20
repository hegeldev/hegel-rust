//! Shrink benchmarks derived from real bugs found by hegel in open-source
//! crates: each test pins the minimal counterexample and a generous budget on
//! post-discovery test-body executions, so shrinker call-count regressions
//! fail loudly without flaking on generation-phase variance. The adversarial
//! branch-switch test additionally pins the shrinker's ability to escape a
//! locally-minimal `one_of` branch, which needs random continuations after
//! lowering the branch selector; every seed is fixed, so the sweep is
//! deterministic.

#[path = "common/mod.rs"]
mod common;

use common::utils::measure_failing_run;
use hegel::generators as gs;

#[test]
fn shrink_budget_integer_boundary() {
    let stats = measure_failing_run(1, 100, |tc| {
        let v = tc.draw(gs::integers::<i64>());
        (v >= 1000).then(|| format!("{v}"))
    });
    assert_eq!(stats.minimal_repr, "1000");
    assert!(
        stats.post_discovery_calls() < 150,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}

#[test]
fn shrink_budget_vec_sum() {
    let stats = measure_failing_run(2, 100, |tc| {
        let v = tc.draw(gs::vecs(gs::integers::<i64>().min_value(0)));
        (v.iter().sum::<i64>() >= 1000).then(|| format!("{v:?}"))
    });
    assert_eq!(stats.minimal_repr, "[1000]");
    assert!(
        stats.post_discovery_calls() < 600,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}

#[test]
fn shrink_budget_nested_vec() {
    let stats = measure_failing_run(4, 100, |tc| {
        let v = tc.draw(gs::vecs(gs::vecs(gs::integers::<i8>())));
        (v.iter().map(|x| x.len()).sum::<usize>() >= 5).then(|| format!("{v:?}"))
    });
    assert_eq!(stats.minimal_repr, "[[0, 0, 0, 0, 0]]");
    assert!(
        stats.post_discovery_calls() < 2000,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}

#[test]
fn shrink_budget_string_containing_bracket() {
    let stats = measure_failing_run(101, 100, |tc| {
        let v = tc.draw(gs::text());
        v.contains(']').then(|| format!("{v:?}"))
    });
    assert_eq!(stats.minimal_repr, "\"]\"");
    assert!(
        stats.post_discovery_calls() < 150,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}

#[test]
fn shrink_budget_vec_of_ranges_with_empty_range() {
    let stats = measure_failing_run(103, 100, |tc| {
        let v = tc.draw(gs::vecs(gs::tuples2(
            gs::integers::<u32>(),
            gs::integers::<u32>(),
        )));
        v.iter().any(|(a, b)| a == b).then(|| format!("{v:?}"))
    });
    assert_eq!(stats.minimal_repr, "[(0, 0)]");
    assert!(
        stats.post_discovery_calls() < 600,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}

#[test]
fn shrink_quality_adversarial_branch_switch() {
    let mut hits = 0;
    for seed in 0..30 {
        let stats = measure_failing_run(seed, 100, |tc| {
            let branch = tc.draw(gs::integers::<u8>().min_value(0).max_value(2));
            if branch == 0 {
                let y = tc.draw(gs::integers::<u32>().min_value(0).max_value(9999));
                (9000..=9500).contains(&y).then(|| format!("A y={y}"))
            } else {
                let a = tc.draw(gs::integers::<i64>());
                (a != 0).then(|| format!("B branch={branch} a={a}"))
            }
        });
        assert!(
            stats.minimal_repr.starts_with("A y=") || stats.minimal_repr.starts_with("B branch="),
            "seed {seed} shrank to an unexpected endpoint: {}",
            stats.minimal_repr
        );
        if stats.minimal_repr == "A y=9000" {
            hits += 1;
        }
        assert!(
            stats.post_discovery_calls() < 12000,
            "seed {seed} took {} post-discovery test-body calls",
            stats.post_discovery_calls()
        );
    }
    assert!(
        hits >= 12,
        "reached the true minimum on only {hits}/30 seeds"
    );
}

#[test]
fn shrink_quality_adversarial_late_branch_divergence() {
    let mut hits = 0;
    for seed in 0..30 {
        let stats = measure_failing_run(seed, 100, |tc| {
            let branch = tc.draw(gs::integers::<u8>().min_value(0).max_value(1));
            if branch == 0 {
                let p = tc.draw(gs::integers::<u8>().min_value(0).max_value(255));
                let y = tc.draw(gs::integers::<u32>().min_value(0).max_value(9999));
                (9000..=9500).contains(&y).then(|| format!("A p={p} y={y}"))
            } else {
                let q = tc.draw(gs::integers::<u8>().min_value(0).max_value(255));
                let a = tc.draw(gs::integers::<i64>());
                (a != 0).then(|| format!("B q={q} a={a}"))
            }
        });
        assert!(
            stats.minimal_repr.starts_with("A p=") || stats.minimal_repr.starts_with("B q="),
            "seed {seed} shrank to an unexpected endpoint: {}",
            stats.minimal_repr
        );
        if stats.minimal_repr == "A p=0 y=9000" {
            hits += 1;
        }
        assert!(
            stats.post_discovery_calls() < 8000,
            "seed {seed} took {} post-discovery test-body calls",
            stats.post_discovery_calls()
        );
    }
    assert!(
        hits >= 13,
        "reached the true minimum on only {hits}/30 seeds"
    );
}

#[test]
fn shrink_budget_singleton_failure_set() {
    let stats = measure_failing_run(102, 5000, |tc| {
        let v = tc.draw(gs::integers::<i64>());
        (v == i64::MIN).then(|| format!("{v}"))
    });
    assert_eq!(stats.minimal_repr, format!("{}", i64::MIN));
    assert!(
        stats.post_discovery_calls() < 150,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}
