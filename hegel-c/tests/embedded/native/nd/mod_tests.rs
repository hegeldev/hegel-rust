//! Embedded tests for `src/native/nd/mod.rs`.
//!
//! The discovery-bar test recomputes the exact DP from experiment 005A over
//! the production decision function and asserts the operating points
//! recorded in decision 23; the gauntlet and budget tests pin the fixture
//! values from experiments 001 and 003.

use super::*;

fn evidence(fails: u64, misses: u64) -> Evidence {
    let mut e = Evidence::default();
    for _ in 0..fails {
        e.record(true, 1.0);
    }
    for _ in 0..misses {
        e.record(false, 1.0);
    }
    e
}

#[test]
fn empty_evidence_has_vacuous_bounds() {
    let e = Evidence::default();
    assert_eq!(e.lower_bound(), 0.0);
    assert_eq!(e.upper_bound(), 1.0);
    assert_eq!(e.fails(), 0);
    assert_eq!(e.runs(), 0);
}

#[test]
fn bounds_bracket_the_observed_rate() {
    let e = evidence(10, 10);
    assert!(e.lower_bound() > 0.0);
    assert!(e.lower_bound() < 0.5);
    assert!(e.upper_bound() > 0.5);
    assert!(e.upper_bound() < 1.0);
    assert_eq!(e.fails(), 10);
    assert_eq!(e.runs(), 20);
}

#[test]
fn certain_failure_clamps_to_the_unit_interval() {
    let e = evidence(10, 0);
    assert!((e.lower_bound() - 0.7225).abs() < 1e-3);
    assert!(e.upper_bound() > 0.999);
}

#[test]
fn diverged_misses_carry_less_weight() {
    let mut full = Evidence::default();
    let mut diverged = Evidence::default();
    for _ in 0..5 {
        full.record(true, 1.0);
        diverged.record(true, 1.0);
    }
    for _ in 0..10 {
        full.record(false, 1.0);
        diverged.record(false, 0.25);
    }
    assert!(diverged.lower_bound() > full.lower_bound());
    assert_eq!(full.runs(), diverged.runs());
}

#[test]
fn bar_accepts_on_the_fourth_failure() {
    for misses in [0, 5, 30] {
        let mut e = evidence(3, misses);
        assert!(matches!(discovery_bar(&e), BarVerdict::Continue));
        e.record(true, 1.0);
        assert!(matches!(discovery_bar(&e), BarVerdict::Accept));
    }
}

#[test]
fn bar_rejects_zero_failures_at_the_gate() {
    assert!(matches!(
        discovery_bar(&evidence(0, 9)),
        BarVerdict::Continue
    ));
    assert!(matches!(
        discovery_bar(&evidence(0, 10)),
        BarVerdict::Reject
    ));
}

#[test]
fn bar_rejects_when_the_cap_is_unreachable() {
    assert!(matches!(
        discovery_bar(&evidence(1, 36)),
        BarVerdict::Continue
    ));
    assert!(matches!(
        discovery_bar(&evidence(1, 37)),
        BarVerdict::Reject
    ));
    assert!(matches!(
        discovery_bar(&evidence(3, 37)),
        BarVerdict::Reject
    ));
}

#[test]
fn diverged_misses_extend_the_gate() {
    let mut e = Evidence::default();
    for _ in 0..10 {
        e.record(false, 0.5);
    }
    assert!(matches!(discovery_bar(&e), BarVerdict::Continue));
    for _ in 0..10 {
        e.record(false, 0.5);
    }
    assert!(matches!(discovery_bar(&e), BarVerdict::Reject));
}

/// Exact DP over [`discovery_bar`] with i.i.d. failure probability `p`:
/// (P(accept), E[replays | reject], E[replays | accept]).
fn bar_operating_point(p: f64) -> (f64, f64, f64) {
    let mut mass = [[0.0f64; 41]; 5];
    mass[0][0] = 1.0;
    let (mut p_accept, mut p_reject) = (0.0, 0.0);
    let (mut replays_accept, mut replays_reject) = (0.0, 0.0);
    for runs in 0..=40usize {
        for fails in 0..5usize {
            let m = mass[fails][runs];
            if m == 0.0 {
                continue;
            }
            match discovery_bar(&evidence(fails as u64, (runs - fails) as u64)) {
                BarVerdict::Accept => {
                    p_accept += m;
                    replays_accept += m * runs as f64;
                }
                BarVerdict::Reject => {
                    p_reject += m;
                    replays_reject += m * runs as f64;
                }
                BarVerdict::Continue => {
                    assert!(runs < 40, "bar must decide at the cap");
                    mass[fails + 1][runs + 1] += m * p;
                    mass[fails][runs + 1] += m * (1.0 - p);
                }
            }
        }
    }
    assert!((p_accept + p_reject - 1.0).abs() < 1e-12);
    (
        p_accept,
        replays_reject / p_reject.max(f64::MIN_POSITIVE),
        replays_accept / p_accept.max(f64::MIN_POSITIVE),
    )
}

#[test]
fn bar_matches_the_decision_23_operating_points() {
    let (false_accept, replays_per_fluke, _) = bar_operating_point(0.02);
    assert!((false_accept - 0.0059).abs() < 0.001);
    assert!((replays_per_fluke - 15.1).abs() < 0.5);

    let (power_at_target, _, _) = bar_operating_point(0.1);
    assert!((power_at_target - 0.454).abs() < 0.01);

    let (confirm_rate, _, replays_per_confirm) = bar_operating_point(0.9);
    assert!(confirm_rate > 0.9999);
    assert!((replays_per_confirm - 4.44).abs() < 0.05);
}

#[test]
fn gauntlet_accepts_consistent_failure_and_reports_the_bound() {
    let mut e = Evidence::default();
    let accepted_bound = loop {
        e.record(true, 1.0);
        match gauntlet(&e, 0.9) {
            GauntletVerdict::Accept { lower_bound } => break lower_bound,
            GauntletVerdict::Continue => {}
            GauntletVerdict::Reject => panic!("certain failure must not be rejected"),
        }
    };
    assert_eq!(e.runs(), 10);
    assert!((accepted_bound - 0.7225).abs() < 1e-3);
}

#[test]
fn gauntlet_rejects_when_the_upper_bound_proves_the_rate_low() {
    let mut e = evidence(1, 0);
    let rejected_at = loop {
        e.record(false, 1.0);
        match gauntlet(&e, 0.9) {
            GauntletVerdict::Reject => break e.runs(),
            GauntletVerdict::Continue => {}
            GauntletVerdict::Accept { .. } => panic!("a rate this low must not be accepted"),
        }
    };
    assert!(rejected_at < GAUNTLET_CAP);
}

#[test]
fn gauntlet_floor_accepts_a_single_failure_at_zero_anchor() {
    let e = evidence(1, 0);
    match gauntlet(&e, 0.0) {
        GauntletVerdict::Accept { lower_bound } => assert!(lower_bound >= GAUNTLET_FLOOR),
        _ => panic!("floor threshold must accept an immediate failure"),
    }
}

#[test]
fn gauntlet_cap_bounds_the_undecidable() {
    let mut e = Evidence::default();
    for i in 0..29 {
        e.record(i % 2 == 0, 1.0);
    }
    assert!(matches!(gauntlet(&e, 0.5), GauntletVerdict::Continue));
    e.record(false, 1.0);
    assert!(matches!(gauntlet(&e, 0.5), GauntletVerdict::Reject));
}

#[test]
fn replay_budget_is_minimal_for_the_tolerance() {
    let b = reuse_replay_budget();
    assert_eq!(b, 29);
    assert!(libm::pow(1.0 - TARGET_FAILURE_RATE, b as f64) <= 0.05);
    assert!(libm::pow(1.0 - TARGET_FAILURE_RATE, (b - 1) as f64) > 0.05);
}

#[test]
fn replay_budget_scales_with_rate() {
    assert_eq!(replay_budget(0.5, 0.05), 5);
    assert!(replay_budget(0.5, 0.05) < replay_budget(0.1, 0.05));
    assert!(replay_budget(0.1, 0.01) > replay_budget(0.1, 0.05));
}

#[test]
fn boost_keep_halves_rounding_up() {
    assert_eq!(boost_keep(16), 8);
    assert_eq!(boost_keep(5), 3);
    assert_eq!(boost_keep(2), 1);
    assert_eq!(boost_keep(1), 1);
}

#[test]
fn continuation_budget_floors_at_four() {
    assert_eq!(continuation_budget(0), 4);
    assert_eq!(continuation_budget(10), 14);
    assert_eq!(continuation_budget(80), 90);
}

#[test]
fn verbatim_weight_is_the_tracked_fraction_of_the_stored_timeline() {
    use crate::native::core::ChoiceValue as CV;
    let stored = alloc::vec![
        CV::Boolean(true),
        CV::Boolean(false),
        CV::Boolean(true),
        CV::Boolean(false),
    ];
    assert_eq!(verbatim_weight(&stored, &stored), 1.0);
    let half = alloc::vec![
        CV::Boolean(true),
        CV::Boolean(false),
        CV::Boolean(false),
        CV::Boolean(true),
    ];
    assert_eq!(verbatim_weight(&stored, &half), 0.5);
    assert_eq!(verbatim_weight(&stored, &stored[..1]), 0.25);
    assert_eq!(verbatim_weight(&stored, &[]), 0.0);
    assert_eq!(verbatim_weight(&stored, &[CV::Boolean(false)]), 0.0);
    assert_eq!(verbatim_weight(&[], &[CV::Boolean(false)]), 1.0);
    let mut longer = stored.clone();
    longer.push(CV::Boolean(true));
    assert_eq!(verbatim_weight(&stored, &longer), 1.0);
}

#[test]
fn failures_count_in_full_at_zero_weight() {
    let mut zero_weight = Evidence::default();
    let mut full_weight = Evidence::default();
    zero_weight.record(true, 0.0);
    full_weight.record(true, 1.0);
    for e in [&mut zero_weight, &mut full_weight] {
        e.record(false, 1.0);
        e.record(false, 0.5);
    }
    assert_eq!(zero_weight.fails(), full_weight.fails());
    assert_eq!(zero_weight.runs(), full_weight.runs());
    assert_eq!(zero_weight.lower_bound(), full_weight.lower_bound());
    assert_eq!(zero_weight.upper_bound(), full_weight.upper_bound());
}

#[test]
fn bar_physical_cap_rejects_diverged_zero_fail_evidence() {
    let mut e = Evidence::default();
    for _ in 0..36 {
        e.record(false, 0.2);
        assert!(matches!(discovery_bar(&e), BarVerdict::Continue));
    }
    e.record(false, 0.2);
    assert!(matches!(discovery_bar(&e), BarVerdict::Reject));
}
