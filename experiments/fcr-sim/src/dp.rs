//! Exact DP over the engine's sequential accept rules on plain counts
//! (decision 71). Rules mirror hegel-c/src/native/nd/mod.rs: the discovery
//! bar (gate 10 / min-fails 4 / cap 40) and the gauntlet
//! (LCB >= threshold with a failure minimum, UCB early reject, cap 30),
//! both checked after every replay.

use std::collections::HashMap;

pub const Z: f64 = 1.96;
pub const GATE_RUNS: u64 = 10;
pub const CONFIRM_CAP: u64 = 40;
pub const CONFIRM_MIN_FAILS: u64 = 4;
pub const GAUNTLET_CAP: u64 = 30;
pub const ANCHOR_SEED_RUNS: u64 = 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Accept,
    Reject,
    Continue,
}

pub fn wilson(fails: f64, runs: f64, upper: bool) -> f64 {
    if runs <= 0.0 {
        return if upper { 1.0 } else { 0.0 };
    }
    let p = fails / runs;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / runs;
    let center = p + z2 / (2.0 * runs);
    let margin = Z * ((p * (1.0 - p) + z2 / (4.0 * runs)) / runs).sqrt();
    let bound = if upper {
        (center + margin) / denom
    } else {
        (center - margin) / denom
    };
    bound.clamp(0.0, 1.0)
}

pub fn bar(n: u64, k: u64) -> Verdict {
    if k >= CONFIRM_MIN_FAILS {
        return Verdict::Accept;
    }
    if k == 0 && n >= GATE_RUNS {
        return Verdict::Reject;
    }
    if k + CONFIRM_CAP.saturating_sub(n) < CONFIRM_MIN_FAILS {
        return Verdict::Reject;
    }
    Verdict::Continue
}

pub fn gauntlet(n: u64, k: u64, threshold: f64, min_fails: u64, cap: u64) -> Verdict {
    if k >= min_fails && wilson(k as f64, n as f64, false) >= threshold {
        return Verdict::Accept;
    }
    if wilson(k as f64, n as f64, true) < threshold || n >= cap {
        return Verdict::Reject;
    }
    Verdict::Continue
}

pub struct DpResult {
    pub p_accept: f64,
    pub e_runs: f64,
    /// (runs, fails, mass) at the moment of acceptance.
    pub accept_states: Vec<(u64, u64, f64)>,
}

/// Walk the (runs, fails) probability mass under per-replay verdict checks,
/// starting from `seed` = (runs, fails, mass) states already on the ledger.
pub fn evaluate(
    rule: impl Fn(u64, u64) -> Verdict,
    p: f64,
    seed: &[(u64, u64, f64)],
    horizon: u64,
) -> DpResult {
    let mut mass: HashMap<(u64, u64), f64> = HashMap::new();
    for &(n, k, m) in seed {
        *mass.entry((n, k)).or_insert(0.0) += m;
    }
    let mut p_accept = 0.0;
    let mut e_runs = 0.0;
    let mut accept_states: HashMap<(u64, u64), f64> = HashMap::new();
    for _ in 0..=horizon {
        let mut next: HashMap<(u64, u64), f64> = HashMap::new();
        for (&(n, k), &m) in &mass {
            match rule(n, k) {
                Verdict::Accept => {
                    p_accept += m;
                    e_runs += m * n as f64;
                    *accept_states.entry((n, k)).or_insert(0.0) += m;
                }
                Verdict::Reject => {
                    e_runs += m * n as f64;
                }
                Verdict::Continue => {
                    *next.entry((n + 1, k + 1)).or_insert(0.0) += m * p;
                    *next.entry((n + 1, k)).or_insert(0.0) += m * (1.0 - p);
                }
            }
        }
        mass = next;
        if mass.is_empty() {
            break;
        }
    }
    assert!(mass.is_empty(), "rule did not terminate within the horizon");
    DpResult {
        p_accept,
        e_runs,
        accept_states: accept_states
            .into_iter()
            .map(|((n, k), m)| (n, k, m))
            .collect(),
    }
}

/// Extend each accepting ledger with unconditioned Bernoulli(p) replays to
/// `total` runs (decision 54's seeding extension) and return the seeded
/// anchor's miscoverage P(LCB > p) and mean LCB, conditioned on acceptance.
pub fn seeded_anchor(states: &[(u64, u64, f64)], p: f64, total: u64) -> (f64, f64) {
    let total_mass: f64 = states.iter().map(|&(_, _, m)| m).sum();
    if total_mass == 0.0 {
        return (0.0, 0.0);
    }
    let mut miscover = 0.0;
    let mut mean_lcb = 0.0;
    for &(n, k, m) in states {
        let extra = total.saturating_sub(n);
        let denom = total.max(n);
        for j in 0..=extra {
            let w = m * binom_pmf(extra, j, p);
            let lcb = wilson((k + j) as f64, denom as f64, false);
            if lcb > p {
                miscover += w;
            }
            mean_lcb += w * lcb;
        }
    }
    (miscover / total_mass, mean_lcb / total_mass)
}

/// Pooled-review replay budget for pool size n: per-timeline first-fit plus
/// splices plus fresh generations (test_runner::nd_reproduce at the final
/// replay).
pub fn review_replays(n: u64) -> u64 {
    n * 29u64.div_ceil(n) + if n >= 2 { 10 } else { 0 } + 4
}

pub fn binom_pmf(n: u64, k: u64, p: f64) -> f64 {
    let mut acc = 1.0f64;
    for i in 0..k {
        acc *= (n - i) as f64 / (i + 1) as f64;
    }
    acc * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32)
}
