//! Experiment 001: shrink-statistics simulation. Spec and results:
//! notes/experiments/001-shrink-sim/notes.md

mod model;
mod sim;
mod stats;

use model::{Candidate, Landscape, ALL_LANDSCAPES, BUG_THRESHOLD};
use sim::{Outcome, Policy, Sim};
use stats::{percentile, Rng};

const SEEDS: u64 = 200;
const START_LEN: usize = 20;
const MIN_BUG_ATOMS: usize = 3;

fn start_candidate(rng: &mut Rng) -> Candidate {
    loop {
        let c: Candidate = (0..START_LEN).map(|_| rng.below(101)).collect();
        if c.iter().filter(|&&a| a >= BUG_THRESHOLD).count() >= MIN_BUG_ATOMS {
            return c;
        }
    }
}

fn run_trials(landscape: Landscape, policy: Policy) -> Vec<Outcome> {
    (0..SEEDS)
        .map(|seed| {
            let mut srng = Rng::new(seed.wrapping_mul(0xA5A5) ^ 0x5EED);
            let start = start_candidate(&mut srng);
            Sim::new(landscape, policy, seed ^ 0xF00D, start).run()
        })
        .collect()
}

struct Row {
    policy: String,
    p_med: f64,
    p_p10: f64,
    p_p90: f64,
    len_med: f64,
    bug_rate: f64,
    execs_med: f64,
    execs_p90: f64,
    accepts_mean: f64,
    grej_mean: f64,
    rollbacks_mean: f64,
    cap_hits: usize,
}

fn summarize(policy: Policy, outs: &[Outcome]) -> Row {
    let mut ps: Vec<f64> = outs.iter().map(|o| o.final_p).collect();
    let mut lens: Vec<f64> = outs.iter().map(|o| o.final_len as f64).collect();
    let mut execs: Vec<f64> = outs.iter().map(|o| o.execs as f64).collect();
    ps.sort_by(f64::total_cmp);
    lens.sort_by(f64::total_cmp);
    execs.sort_by(f64::total_cmp);
    let n = outs.len() as f64;
    Row {
        policy: policy.name(),
        p_med: percentile(&ps, 0.5),
        p_p10: percentile(&ps, 0.1),
        p_p90: percentile(&ps, 0.9),
        len_med: percentile(&lens, 0.5),
        bug_rate: outs.iter().filter(|o| o.bug_retained).count() as f64 / n,
        execs_med: percentile(&execs, 0.5),
        execs_p90: percentile(&execs, 0.9),
        accepts_mean: outs.iter().map(|o| o.accepts as f64).sum::<f64>() / n,
        grej_mean: outs.iter().map(|o| o.gauntlet_rejects as f64).sum::<f64>() / n,
        rollbacks_mean: outs.iter().map(|o| o.rollbacks as f64).sum::<f64>() / n,
        cap_hits: outs.iter().filter(|o| o.hit_cap).count(),
    }
}

fn print_table(title: &str, rows: &[Row]) {
    println!("\n### {title}\n");
    println!(
        "| policy | final p med (p10-p90) | len med | bug kept | execs med (p90) | accepts | g-rej | rollbacks | cap hits |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for r in rows {
        println!(
            "| {} | {:.2} ({:.2}-{:.2}) | {:.0} | {:.0}% | {:.0} ({:.0}) | {:.1} | {:.1} | {:.2} | {} |",
            r.policy,
            r.p_med,
            r.p_p10,
            r.p_p90,
            r.len_med,
            r.bug_rate * 100.0,
            r.execs_med,
            r.execs_p90,
            r.accepts_mean,
            r.grej_mean,
            r.rollbacks_mean,
            r.cap_hits,
        );
    }
}

fn main() {
    let policies = [
        Policy::Naive,
        Policy::PerCandidateN { n: 10 },
        Policy::FixedGauntlet { m: 5 },
        Policy::Ledger { gamma: 0.8, checkpoint: false },
        Policy::Ledger { gamma: 0.8, checkpoint: true },
    ];

    println!("# shrink-sim results ({SEEDS} seeds per cell)");
    for landscape in ALL_LANDSCAPES {
        let rows: Vec<Row> = policies
            .iter()
            .map(|&p| summarize(p, &run_trials(landscape, p)))
            .collect();
        print_table(landscape.name(), &rows);
    }

    let gammas = [0.5, 0.8, 1.0];
    for landscape in [Landscape::RisingWithSize, Landscape::NoiseFloor] {
        let rows: Vec<Row> = gammas
            .iter()
            .map(|&gamma| {
                let p = Policy::Ledger { gamma, checkpoint: false };
                summarize(p, &run_trials(landscape, p))
            })
            .collect();
        print_table(&format!("gamma sensitivity, {}", landscape.name()), &rows);
    }
}
