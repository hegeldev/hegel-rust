use crate::landscape::Landscape;
use crate::stats::{median_upper, wilson_lcb, Rng};

const FIRINGS: u64 = 3;
const POOL_SIZE: usize = 16;
const POOL_ATTEMPTS: u64 = 48;
const Z: f64 = 1.96;

pub struct NewOutcome {
    pub final_x: i64,
    pub progress: f64,
    pub adopts: u64,
    pub ref_drift: f64,
    pub replays: u64,
    pub dead: bool,
}

struct Cand {
    x: i64,
    sum: f64,
    n: u64,
}

impl Cand {
    fn mean(&self) -> f64 {
        if self.n == 0 {
            f64::NEG_INFINITY
        } else {
            self.sum / self.n as f64
        }
    }
}

fn batch(
    land: Landscape,
    x: i64,
    miss: f64,
    holdout: u64,
    rng: &mut Rng,
    replays: &mut u64,
) -> Vec<f64> {
    let mut v = Vec::new();
    for _ in 0..holdout {
        *replays += 1;
        if let Some(s) = land.run(x, rng, miss) {
            v.push(s);
        }
    }
    v
}

pub fn run_trial(land: Landscape, miss: f64, holdout: u64, races: u64, seed: u64) -> NewOutcome {
    let mut rng = Rng::new(seed);
    let mut replays: u64 = 0;
    let b0 = batch(land, 0, miss, holdout, &mut rng, &mut replays);
    if b0.is_empty() {
        return NewOutcome {
            final_x: 0,
            progress: 0.0,
            adopts: 0,
            ref_drift: f64::NAN,
            replays,
            dead: true,
        };
    }
    let mut r = median_upper(b0);
    let mut pos: i64 = 0;
    let mut adopts: u64 = 0;
    for _ in 0..FIRINGS {
        'race: for _ in 0..races {
            let mut pool: Vec<i64> = Vec::new();
            let mut attempts = 0;
            while pool.len() < POOL_SIZE && attempts < POOL_ATTEMPTS {
                attempts += 1;
                let step = 1i64 << rng.below(7);
                let raw = if rng.below(2) == 0 { pos + step } else { pos - step };
                let c = raw.clamp(0, 100);
                if !pool.contains(&c) {
                    pool.push(c);
                }
            }
            let mut cands: Vec<Cand> = pool
                .into_iter()
                .map(|x| Cand { x, sum: 0.0, n: 0 })
                .collect();
            let mut per_round: u64 = 2;
            while cands.len() > 1 {
                for c in &mut cands {
                    for _ in 0..per_round {
                        replays += 1;
                        if let Some(s) = land.run(c.x, &mut rng, miss) {
                            c.sum += s;
                            c.n += 1;
                        }
                    }
                }
                cands.sort_by(|a, b| {
                    b.mean()
                        .partial_cmp(&a.mean())
                        .unwrap()
                        .then(a.x.cmp(&b.x))
                });
                cands.truncate(cands.len().div_ceil(2));
                per_round *= 2;
            }
            let winner = &cands[0];
            if winner.n == 0 {
                break 'race;
            }
            let wx = winner.x;
            let mut beats: u64 = 0;
            for _ in 0..holdout {
                replays += 1;
                if let Some(s) = land.run(wx, &mut rng, miss) {
                    if s > r {
                        beats += 1;
                    }
                }
            }
            if wilson_lcb(beats, holdout, Z) > 0.5 {
                adopts += 1;
                let b = batch(land, wx, miss, holdout, &mut rng, &mut replays);
                if !b.is_empty() {
                    r = r.max(median_upper(b));
                }
                pos = wx;
            }
        }
    }
    NewOutcome {
        final_x: pos,
        progress: land.true_mean(pos) - land.true_mean(0),
        adopts,
        ref_drift: r - land.true_median_x0(),
        replays,
        dead: false,
    }
}
