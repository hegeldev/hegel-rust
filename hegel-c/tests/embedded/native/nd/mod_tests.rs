//! Embedded tests for `src/native/nd/mod.rs`.
//!
//! The discovery-bar and gauntlet tests recompute the exact DPs from
//! experiments 005A and 008 over the production decision functions and
//! assert the recorded operating points (decisions 23 and 54); the budget
//! tests pin the replay budgets decisions 11 and 16 derive and experiment
//! 004's continuation budget.

use super::*;

fn evidence(fails: u64, misses: u64) -> Evidence {
    let mut e = Evidence::default();
    for _ in 0..fails {
        e.record(true);
    }
    for _ in 0..misses {
        e.record(false);
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
fn bar_accepts_on_the_fourth_failure() {
    for misses in [0, 5, 30] {
        let mut e = evidence(3, misses);
        assert!(matches!(discovery_bar(&e), BarVerdict::Continue));
        e.record(true);
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
fn gauntlet_accepts_consistent_failure_at_the_evidence_bound() {
    let mut e = Evidence::default();
    loop {
        e.record(true);
        match gauntlet(&e, 0.79) {
            GauntletVerdict::Accept => break,
            GauntletVerdict::Continue => {}
            GauntletVerdict::Reject => panic!("certain failure must not be rejected"),
        }
    }
    assert_eq!(e.runs(), 7);
    assert!((e.lower_bound() - 0.6455).abs() < 1e-3);
}

#[test]
fn gauntlet_rejects_when_the_upper_bound_proves_the_rate_low() {
    let mut e = evidence(1, 0);
    let rejected_at = loop {
        e.record(false);
        match gauntlet(&e, 0.9) {
            GauntletVerdict::Reject => break e.runs(),
            GauntletVerdict::Continue => {}
            GauntletVerdict::Accept => panic!("a rate this low must not be accepted"),
        }
    };
    assert!(rejected_at < GAUNTLET_CAP);
}

#[test]
fn gauntlet_never_accepts_below_minimum_evidence() {
    for fails in 1..GAUNTLET_MIN_FAILS {
        assert!(
            matches!(
                gauntlet(&evidence(fails, 0), 0.0),
                GauntletVerdict::Continue
            ),
            "{fails} straight failures are still short of the evidence minimum"
        );
    }
    assert!(matches!(
        gauntlet(&evidence(GAUNTLET_MIN_FAILS, 0), 0.0),
        GauntletVerdict::Accept
    ));
}

#[test]
fn gauntlet_floor_matches_its_derivation() {
    let boundary = evidence(GAUNTLET_MIN_FAILS, GAUNTLET_CAP - GAUNTLET_MIN_FAILS);
    assert!((boundary.lower_bound() - 0.0531).abs() < 5e-4);
    assert!(GAUNTLET_FLOOR < boundary.lower_bound());
    assert!(matches!(gauntlet(&boundary, 0.0), GauntletVerdict::Accept));
}

#[test]
fn gauntlet_gamma_is_unity_above_the_retention_high_water() {
    let e = evidence(18, 2);
    assert!(e.lower_bound() > 0.69);
    assert!(matches!(gauntlet(&e, 0.79), GauntletVerdict::Accept));
    assert!(matches!(
        gauntlet(&e, RETENTION_HIGH_WATER),
        GauntletVerdict::Continue
    ));
    assert!(matches!(gauntlet(&e, 0.83), GauntletVerdict::Continue));
}

/// Exact DP over [`gauntlet`] with i.i.d. failure probability `q` for one
/// fresh-ledger candidate whose recruiting run failed and is counted
/// (decision 54): (P(accept | fail), E[physical runs | fail]).
fn gauntlet_operating_point(q: f64, anchor: f64) -> (f64, f64) {
    let cap = GAUNTLET_CAP as usize;
    let mut mass = [[0.0f64; 31]; 31];
    mass[1][1] = 1.0;
    let mut p_accept = 0.0;
    let mut runs_total = 0.0;
    for runs in 1..=cap {
        for fails in 1..=runs {
            let m = mass[fails][runs];
            if m == 0.0 {
                continue;
            }
            match gauntlet(&evidence(fails as u64, (runs - fails) as u64), anchor) {
                GauntletVerdict::Accept => {
                    p_accept += m;
                    runs_total += m * runs as f64;
                }
                GauntletVerdict::Reject => {
                    runs_total += m * runs as f64;
                }
                GauntletVerdict::Continue => {
                    assert!(runs < cap, "the gauntlet must decide at the cap");
                    mass[fails + 1][runs + 1] += m * q;
                    mass[fails][runs + 1] += m * (1.0 - q);
                }
            }
        }
    }
    (p_accept, runs_total)
}

#[test]
fn gauntlet_matches_the_008_operating_points() {
    let rows = [
        (0.05, 0.02, 0.0198, 29.9),
        (0.05, 0.10, 0.5650, 24.2),
        (0.05, 0.90, 1.0000, 4.3),
        (0.30, 0.02, 0.00016, 22.5),
        (0.30, 0.10, 0.0182, 27.8),
        (0.30, 0.90, 1.0000, 4.3),
        (0.839, 0.02, 0.0, 3.1),
        (0.839, 0.10, 0.0, 3.4),
        (0.839, 0.90, 0.1216, 28.3),
    ];
    for (anchor, q, p_accept, runs) in rows {
        let (p, r) = gauntlet_operating_point(q, anchor);
        assert!(
            (p - p_accept).abs() < 5e-4,
            "anchor {anchor} q {q}: P(accept | fail) {p}, expected {p_accept}"
        );
        assert!(
            (r - runs).abs() < 0.05,
            "anchor {anchor} q {q}: E[runs | fail] {r}, expected {runs}"
        );
    }
}

#[test]
fn constants_match_their_documented_values() {
    assert_eq!(GATE_RUNS, 10);
    assert_eq!(CONFIRM_CAP, 40);
    assert_eq!(CONFIRM_MIN_FAILS, 4);
    assert_eq!(GAUNTLET_CAP, 30);
    assert_eq!(GAUNTLET_MIN_FAILS, 4);
    assert_eq!(GAUNTLET_GAMMA, 0.8);
    assert_eq!(GAUNTLET_FLOOR, 0.05);
    assert_eq!(RETENTION_HIGH_WATER, 0.8);
    assert_eq!(ANCHOR_SEED_RUNS, 20);
    assert_eq!(BOOST_HOLDOUT, ANCHOR_SEED_RUNS);
    assert_eq!(BOOST_RELIABILITY_FLOOR, 0.30);
    assert_eq!(POOL_CAP, 10);
    assert_eq!(BOOST_POOL, 16);
    assert_eq!(REPRODUCE_SPLICES, 10);
    assert_eq!(FINAL_REPLAY_FRESH, 4);
    assert_eq!(TARGET_FAILURE_RATE, 0.1);
}

#[test]
fn gauntlet_cap_bounds_the_undecidable() {
    let mut e = Evidence::default();
    for i in 0..29 {
        e.record(i % 2 == 0);
    }
    assert!(matches!(gauntlet(&e, 0.5), GauntletVerdict::Continue));
    e.record(false);
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

/// The sign test's acceptance boundary at the holdout size: 15 of 20 is
/// the smallest beat count whose Wilson lower bound clears 0.5
/// (experiment 013's DP table).
#[test]
fn target_adopt_needs_fifteen_of_twenty_beats() {
    assert!(!target_adopt(14, TARGET_ND_HOLDOUT));
    assert!(target_adopt(15, TARGET_ND_HOLDOUT));
    assert!(target_adopt(20, TARGET_ND_HOLDOUT));
    assert!(!target_adopt(0, TARGET_ND_HOLDOUT));
    assert!(!target_adopt(0, 0));
}

#[test]
fn target_median_takes_the_upper_middle_of_the_sorted_scores() {
    assert_eq!(target_median(&[]), None);
    assert_eq!(target_median(&[3.0]), Some(3.0));
    assert_eq!(target_median(&[2.0, 1.0, 3.0]), Some(2.0));
    assert_eq!(target_median(&[4.0, 1.0, 2.0, 3.0]), Some(3.0));
}

#[test]
fn continuation_budget_floors_at_four() {
    assert_eq!(continuation_budget(0), 4);
    assert_eq!(continuation_budget(10), 14);
    assert_eq!(continuation_budget(80), 90);
}
