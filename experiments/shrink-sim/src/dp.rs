//! Exact DP over (physical, fails) gauntlet states — the 005A method —
//! giving per-candidate operating points for a candidate at true rate q
//! against anchor a, under the same verdict function the simulator runs.

use crate::e008::{gauntlet_verdict, Config, Verdict, WEvidence, GAUNTLET_CAP};

pub struct DpResult {
    pub accept_given_fail: f64,
    pub accept_unconditional: f64,
    pub runs_given_fail: f64,
}

fn state(physical: u64, fails: u64, miss_weight: f64) -> WEvidence {
    WEvidence {
        fails,
        physical,
        weighted_misses: miss_weight * (physical - fails) as f64,
    }
}

fn solve(
    physical: u64,
    fails: u64,
    q: f64,
    anchor: f64,
    cfg: &Config,
    memo: &mut Vec<Option<(f64, f64)>>,
) -> (f64, f64) {
    let cap = GAUNTLET_CAP + 1;
    let idx = (physical * (cap + 1) + fails) as usize;
    if let Some(v) = memo[idx] {
        return v;
    }
    let e = state(physical, fails, cfg.miss_weight);
    let out = match gauntlet_verdict(&e, anchor, cfg) {
        Verdict::Accept => (1.0, 0.0),
        Verdict::Reject => (0.0, 0.0),
        Verdict::Continue => {
            let (pf, rf) = solve(physical + 1, fails + 1, q, anchor, cfg, memo);
            let (pm, rm) = solve(physical + 1, fails, q, anchor, cfg, memo);
            (
                q * pf + (1.0 - q) * pm,
                1.0 + q * rf + (1.0 - q) * rm,
            )
        }
    };
    memo[idx] = Some(out);
    out
}

pub fn gauntlet_dp(q: f64, anchor: f64, cfg: &Config) -> DpResult {
    let cap = GAUNTLET_CAP + 1;
    let mut memo: Vec<Option<(f64, f64)>> = vec![None; ((cap + 1) * (cap + 1) + cap + 1) as usize];
    let (start_p, start_f) = if cfg.rule.exclude_recruit { (0, 0) } else { (1, 1) };
    let (p, r) = solve(start_p, start_f, q, anchor, cfg, &mut memo);
    DpResult {
        accept_given_fail: p,
        accept_unconditional: q * p,
        runs_given_fail: 1.0 + r,
    }
}
