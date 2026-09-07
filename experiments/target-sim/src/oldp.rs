use crate::landscape::Landscape;
use crate::stats::Rng;

pub const BUDGET: u64 = 600;

pub struct OldOutcome {
    pub final_x: i64,
    pub true_mean: f64,
    pub curse_bias: f64,
    pub frozen: bool,
    pub steps: u64,
    pub runs: u64,
}

pub fn run_trial(land: Landscape, miss: f64, seed: u64) -> OldOutcome {
    let mut rng = Rng::new(seed);
    let mut used: u64 = 0;
    let mut x_best: i64 = 0;
    let mut s_best = f64::NEG_INFINITY;
    let mut steps: u64 = 0;
    let mut late_steps: u64 = 0;
    let mut consec_failed_cycles = 0;
    if let Some(s) = land.run(0, &mut rng, miss) {
        s_best = s;
    }
    used += 1;
    'climb: while consec_failed_cycles < 2 {
        let mut any_first_succeeded = false;
        for dir in [1i64, -1] {
            let mut delta: i64 = 1;
            let mut first = true;
            loop {
                if used >= BUDGET {
                    break 'climb;
                }
                let xp = (x_best + dir * delta).clamp(0, 100);
                used += 1;
                match land.run(xp, &mut rng, miss) {
                    Some(s) if s > s_best => {
                        if first {
                            any_first_succeeded = true;
                        }
                        x_best = xp;
                        s_best = s;
                        steps += 1;
                        if used > BUDGET / 2 {
                            late_steps += 1;
                        }
                        delta = (delta * 2).min(256);
                    }
                    _ => break,
                }
                first = false;
            }
        }
        if any_first_succeeded {
            consec_failed_cycles = 0;
        } else {
            consec_failed_cycles += 1;
        }
    }
    let true_mean = land.true_mean(x_best);
    OldOutcome {
        final_x: x_best,
        true_mean,
        curse_bias: s_best - true_mean,
        frozen: late_steps == 0 && land.has_gradient_above(x_best),
        steps,
        runs: used,
    }
}
