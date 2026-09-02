//! Experiment 004: replay semantics on structurally nondeterministic bodies.
//! Spec and results: notes/experiments/004-replay-semantics/notes.md

use std::cell::RefCell;

use hegel_c::__bench::{
    replay_once, BigInt, ChoiceValue, DataSource, Failure, TestCaseResult, ToPrimitive,
};

const TRIALS: u64 = 40;
const DISCOVERY_CAP: u64 = 400;
const CONFIRM_RUNS: usize = 20;
const POOL_BUILD_RUNS: usize = 40;
const R: usize = 50;
const POOL_CAP: usize = 20;
const KS: [usize; 5] = [1, 2, 5, 10, 20];
const EXTENDS: [usize; 4] = [0, 4, 16, 64];
const POOL_EXTEND: usize = 16;
const FRESH_EXTEND: usize = 64;
const BUG_ATOM: i64 = 90;
const BUG_COUNT: usize = 3;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1))
    }

    fn f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Body {
    Det,
    LateCoin,
    StepCoins,
    KindFlip,
    StablePrefix,
    HetShift,
}

const ALL_BODIES: [Body; 6] = [
    Body::Det,
    Body::LateCoin,
    Body::StepCoins,
    Body::KindFlip,
    Body::StablePrefix,
    Body::HetShift,
];

impl Body {
    fn name(self) -> &'static str {
        match self {
            Body::Det => "B0 det",
            Body::LateCoin => "B1 late-coin",
            Body::StepCoins => "B2 step-coins",
            Body::KindFlip => "B3 kind-flip",
            Body::StablePrefix => "B4 stable-prefix",
            Body::HetShift => "B5 het-shift",
        }
    }
}

fn body_draws(body: Body, hidden: &mut Rng, ds: &dyn DataSource) -> Result<Vec<i64>, ()> {
    let int = |lo: i64, hi: i64| -> Result<i64, ()> {
        ds.generate_integer(&BigInt::from(lo), &BigInt::from(hi))
            .map(|v| v.to_i64().unwrap())
            .map_err(|_| ())
    };
    let coin = || ds.generate_boolean(0.5, None).map(|_| ()).map_err(|_| ());
    let mut atoms = Vec::new();
    match body {
        Body::Det => {
            for _ in 0..int(0, 12)? {
                atoms.push(int(0, 100)?);
            }
        }
        Body::LateCoin => {
            for _ in 0..int(0, 12)? {
                atoms.push(int(0, 100)?);
            }
            if hidden.f64() < 0.3 {
                coin()?;
            }
        }
        Body::StepCoins => {
            for _ in 0..int(0, 10)? {
                atoms.push(int(0, 100)?);
                if hidden.f64() < 0.2 {
                    atoms.push(int(0, 100)?);
                }
            }
        }
        Body::KindFlip => {
            for _ in 0..int(0, 10)? {
                if hidden.f64() < 0.15 {
                    coin()?;
                } else {
                    atoms.push(int(0, 100)?);
                }
            }
        }
        Body::StablePrefix => {
            for _ in 0..4 {
                atoms.push(int(0, 100)?);
            }
            for _ in 0..int(0, 8)? {
                atoms.push(int(0, 100)?);
                if hidden.f64() < 0.2 {
                    atoms.push(int(0, 100)?);
                }
            }
        }
        Body::HetShift => {
            for _ in 0..int(0, 10)? {
                atoms.push(int(0, 100)?);
                int(0, 1)?;
                if hidden.f64() < 0.2 {
                    int(0, 1)?;
                }
            }
        }
    }
    Ok(atoms)
}

fn run(
    body: Body,
    hidden: &RefCell<Rng>,
    choices: &[ChoiceValue],
    extend: usize,
    seed: u64,
) -> (bool, Vec<ChoiceValue>) {
    let out = replay_once(choices, extend, seed, |ds| {
        match body_draws(body, &mut hidden.borrow_mut(), ds.as_ref()) {
            Err(()) => ds.mark_complete(&TestCaseResult::Overrun),
            Ok(atoms) => {
                let bug = atoms.iter().filter(|&&a| a >= BUG_ATOM).count() >= BUG_COUNT;
                let result = if bug {
                    TestCaseResult::Interesting(Failure {
                        origin: "bug".to_string(),
                        reproduce_blob: None,
                    })
                } else {
                    TestCaseResult::Valid
                };
                ds.mark_complete(&result);
            }
        }
    })
    .unwrap();
    (out.interesting, out.realized)
}

fn lcp(a: &[ChoiceValue], b: &[ChoiceValue]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

struct TrialResult {
    discovery_attempts: u64,
    confirm_fails: usize,
    watermarks: Vec<f64>,
    pool_size: usize,
    pool_pair_lcp: Option<f64>,
    repro_by_extend: [usize; 4],
    pool_hits: [usize; 5],
    pool_replays: [usize; 5],
    splice_attempts: usize,
    splice_rescues: usize,
    splice_replays: usize,
    fresh_hits: usize,
}

const SPLICE_TRIES: usize = 10;
const SPLICE_BASE_K: usize = 10;

fn run_trial(body: Body, trial: u64) -> Option<TrialResult> {
    let hidden = RefCell::new(Rng::new(trial.wrapping_mul(0xC0FFEE) ^ 0xD15EA5E));
    let mut seed = trial.wrapping_mul(1_000_003);
    let mut next_seed = || {
        seed += 1;
        seed
    };

    let mut t0 = None;
    let mut discovery_attempts = 0;
    for _ in 0..DISCOVERY_CAP {
        discovery_attempts += 1;
        let (interesting, realized) = run(body, &hidden, &[], FRESH_EXTEND, next_seed());
        if interesting {
            t0 = Some(realized);
            break;
        }
    }
    let t0 = t0?;

    let mut pool: Vec<Vec<ChoiceValue>> = vec![t0.clone()];
    let mut confirm_fails = 0;
    let mut watermarks = Vec::new();
    for _ in 0..CONFIRM_RUNS {
        let (interesting, realized) = run(body, &hidden, &t0, POOL_EXTEND, next_seed());
        watermarks.push(lcp(&t0, &realized) as f64 / t0.len().max(1) as f64);
        if interesting {
            confirm_fails += 1;
            if pool.len() < POOL_CAP && !pool.contains(&realized) {
                pool.push(realized);
            }
        }
    }

    for _ in 0..POOL_BUILD_RUNS {
        let (interesting, realized) = run(body, &hidden, &t0, POOL_EXTEND, next_seed());
        if interesting && pool.len() < POOL_CAP && !pool.contains(&realized) {
            pool.push(realized);
        }
    }

    let mut pair_lcps = Vec::new();
    for i in 0..pool.len() {
        for j in i + 1..pool.len() {
            let denom = pool[i].len().min(pool[j].len()).max(1);
            pair_lcps.push(lcp(&pool[i], &pool[j]) as f64 / denom as f64);
        }
    }
    let pool_pair_lcp = (!pair_lcps.is_empty())
        .then(|| pair_lcps.iter().sum::<f64>() / pair_lcps.len() as f64);

    let mut repro_by_extend = [0usize; 4];
    for (slot, &extend) in EXTENDS.iter().enumerate() {
        for _ in 0..R {
            if run(body, &hidden, &t0, extend, next_seed()).0 {
                repro_by_extend[slot] += 1;
            }
        }
    }

    let mut pool_hits = [0usize; 5];
    let mut pool_replays = [0usize; 5];
    for (slot, &k) in KS.iter().enumerate() {
        let sub = &pool[..k.min(pool.len())];
        for _ in 0..R {
            for entry in sub {
                pool_replays[slot] += 1;
                if run(body, &hidden, entry, POOL_EXTEND, next_seed()).0 {
                    pool_hits[slot] += 1;
                    break;
                }
            }
        }
    }

    let mut splice_attempts = 0;
    let mut splice_rescues = 0;
    let mut splice_replays = 0;
    let base = &pool[..SPLICE_BASE_K.min(pool.len())];
    let mut splice_rng = Rng::new(trial ^ 0x5EA5_1DE5);
    for _ in 0..R {
        let mut hit = false;
        for entry in base {
            if run(body, &hidden, entry, POOL_EXTEND, next_seed()).0 {
                hit = true;
                break;
            }
        }
        if hit || base.len() < 2 {
            continue;
        }
        splice_attempts += 1;
        for _ in 0..SPLICE_TRIES {
            let i = (splice_rng.f64() * base.len() as f64) as usize % base.len();
            let mut j = (splice_rng.f64() * base.len() as f64) as usize % base.len();
            if j == i {
                j = (j + 1) % base.len();
            }
            let max_w = base[i].len().min(base[j].len());
            if max_w < 2 {
                continue;
            }
            let w = 1 + (splice_rng.f64() * (max_w - 1) as f64) as usize % (max_w - 1);
            let mut candidate = base[i][..w].to_vec();
            candidate.extend_from_slice(&base[j][w..]);
            splice_replays += 1;
            if run(body, &hidden, &candidate, POOL_EXTEND, next_seed()).0 {
                splice_rescues += 1;
                break;
            }
        }
    }

    let mut fresh_hits = 0;
    for _ in 0..R {
        if run(body, &hidden, &[], FRESH_EXTEND, next_seed()).0 {
            fresh_hits += 1;
        }
    }

    Some(TrialResult {
        discovery_attempts,
        confirm_fails,
        watermarks,
        pool_size: pool.len(),
        pool_pair_lcp,
        repro_by_extend,
        pool_hits,
        pool_replays,
        splice_attempts,
        splice_rescues,
        splice_replays,
        fresh_hits,
    })
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn main() {
    println!(
        "# replay-semantics results ({TRIALS} trials/body, {R} attempts/strategy, \
         bug = >= {BUG_COUNT} atoms >= {BUG_ATOM})\n"
    );
    let mut all: Vec<(Body, Vec<TrialResult>)> = Vec::new();
    for body in ALL_BODIES {
        let results: Vec<TrialResult> =
            (0..TRIALS).filter_map(|t| run_trial(body, t)).collect();
        all.push((body, results));
    }

    println!("## Discovery, confirmation, watermarks, pool shape\n");
    println!("| body | trials | disc med | fresh % | confirm med /20 | watermark p50 (p10) | pool med | pair-lcp mean |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for (body, results) in &all {
        let n = results.len();
        let mut disc: Vec<f64> = results.iter().map(|r| r.discovery_attempts as f64).collect();
        disc.sort_by(f64::total_cmp);
        let fresh = results.iter().map(|r| r.fresh_hits).sum::<usize>() as f64
            / (n * R).max(1) as f64;
        let mut confirms: Vec<f64> = results.iter().map(|r| r.confirm_fails as f64).collect();
        confirms.sort_by(f64::total_cmp);
        let mut marks: Vec<f64> = results.iter().flat_map(|r| r.watermarks.clone()).collect();
        marks.sort_by(f64::total_cmp);
        let mut pools: Vec<f64> = results.iter().map(|r| r.pool_size as f64).collect();
        pools.sort_by(f64::total_cmp);
        let lcps: Vec<f64> = results.iter().filter_map(|r| r.pool_pair_lcp).collect();
        let lcp_mean = if lcps.is_empty() {
            "n/a".to_string()
        } else {
            format!("{:.2}", lcps.iter().sum::<f64>() / lcps.len() as f64)
        };
        println!(
            "| {} | {}/{} | {:.0} | {:.1} | {:.0} | {:.2} ({:.2}) | {:.0} | {} |",
            body.name(),
            n,
            TRIALS,
            percentile(&disc, 0.5),
            100.0 * fresh,
            percentile(&confirms, 0.5),
            percentile(&marks, 0.5),
            percentile(&marks, 0.1),
            percentile(&pools, 0.5),
            lcp_mean,
        );
    }

    println!("\n## Reproduction rate: single timeline by extend budget (% of {R} cold attempts)\n");
    println!("| body | e=0 | e=4 | e=16 | e=64 |");
    println!("| --- | --- | --- | --- | --- |");
    for (body, results) in &all {
        let n = results.len();
        let rate = |f: &dyn Fn(&TrialResult) -> usize| {
            100.0 * results.iter().map(f).sum::<usize>() as f64 / (n * R).max(1) as f64
        };
        println!(
            "| {} | {:.0} | {:.0} | {:.0} | {:.0} |",
            body.name(),
            rate(&|r| r.repro_by_extend[0]),
            rate(&|r| r.repro_by_extend[1]),
            rate(&|r| r.repro_by_extend[2]),
            rate(&|r| r.repro_by_extend[3]),
        );
    }

    println!("\n## Reproduction rate: pool first-fit by pool cap K (e={POOL_EXTEND}; replays/attempt in parens)\n");
    println!("| body | K=1 | K=2 | K=5 | K=10 | K=20 |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for (body, results) in &all {
        let n = results.len();
        let cells: Vec<String> = (0..KS.len())
            .map(|slot| {
                let hits = results.iter().map(|r| r.pool_hits[slot]).sum::<usize>();
                let replays = results.iter().map(|r| r.pool_replays[slot]).sum::<usize>();
                let attempts = (n * R).max(1);
                format!(
                    "{:.0} ({:.1})",
                    100.0 * hits as f64 / attempts as f64,
                    replays as f64 / attempts as f64
                )
            })
            .collect();
        println!("| {} | {} |", body.name(), cells.join(" | "));
    }

    println!("\n## Splice fallback after a full pool miss (K={SPLICE_BASE_K}, {SPLICE_TRIES} positional splices, e={POOL_EXTEND})\n");
    println!("| body | pool misses | rescued | rescue % | splice replays/miss |");
    println!("| --- | --- | --- | --- | --- |");
    for (body, results) in &all {
        let misses = results.iter().map(|r| r.splice_attempts).sum::<usize>();
        let rescues = results.iter().map(|r| r.splice_rescues).sum::<usize>();
        let replays = results.iter().map(|r| r.splice_replays).sum::<usize>();
        println!(
            "| {} | {} | {} | {} | {} |",
            body.name(),
            misses,
            rescues,
            if misses == 0 {
                "n/a".to_string()
            } else {
                format!("{:.0}", 100.0 * rescues as f64 / misses as f64)
            },
            if misses == 0 {
                "n/a".to_string()
            } else {
                format!("{:.1}", replays as f64 / misses as f64)
            },
        );
    }
}
