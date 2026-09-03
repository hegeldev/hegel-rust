//! Experiment 001: shrink-statistics simulation (default run). Spec and
//! results: notes/experiments/001-shrink-sim/notes.md
//! Experiment 008 subcommands (e008-*): gauntlet calibration under the
//! shipped rules. Spec and results:
//! notes/experiments/008-gauntlet-calibration/notes.md

mod cmds;
mod dp;
mod e008;
mod model;
mod sim;
mod stats;

use model::{Candidate, Landscape, Pin, ALL_LANDSCAPES, BUG_THRESHOLD};
use sim::{Outcome, Policy, Sim, Stopping};
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

fn run_trials(landscape: Landscape, policy: Policy, stopping: Stopping) -> Vec<Outcome> {
    (0..SEEDS)
        .map(|seed| {
            let mut srng = Rng::new(seed.wrapping_mul(0xA5A5) ^ 0x5EED);
            let start = start_candidate(&mut srng);
            Sim::new(landscape, policy, stopping, seed ^ 0xF00D, start).run()
        })
        .collect()
}

struct Row {
    label: String,
    p_med: f64,
    p_p10: f64,
    p_p90: f64,
    eff_med: f64,
    eff_p10: f64,
    eff_p90: f64,
    len_med: f64,
    bug_rate: f64,
    missed_rate: f64,
    execs_med: f64,
    execs_p90: f64,
    accepts_mean: f64,
    grej_mean: f64,
    rollbacks_mean: f64,
    cap_hits: usize,
}

fn summarize(label: String, outs: &[Outcome]) -> Row {
    let mut ps: Vec<f64> = outs.iter().map(|o| o.final_p).collect();
    let mut effs: Vec<f64> = outs.iter().map(|o| o.eff_p).collect();
    let mut lens: Vec<f64> = outs.iter().map(|o| o.final_len as f64).collect();
    let mut execs: Vec<f64> = outs.iter().map(|o| o.execs as f64).collect();
    ps.sort_by(f64::total_cmp);
    effs.sort_by(f64::total_cmp);
    lens.sort_by(f64::total_cmp);
    execs.sort_by(f64::total_cmp);
    let n = outs.len() as f64;
    Row {
        label,
        p_med: percentile(&ps, 0.5),
        p_p10: percentile(&ps, 0.1),
        p_p90: percentile(&ps, 0.9),
        eff_med: percentile(&effs, 0.5),
        eff_p10: percentile(&effs, 0.1),
        eff_p90: percentile(&effs, 0.9),
        len_med: percentile(&lens, 0.5),
        bug_rate: outs.iter().filter(|o| o.bug_retained).count() as f64 / n,
        missed_rate: outs.iter().filter(|o| o.missed).count() as f64 / n,
        execs_med: percentile(&execs, 0.5),
        execs_p90: percentile(&execs, 0.9),
        accepts_mean: outs.iter().map(|o| o.accepts as f64).sum::<f64>() / n,
        grej_mean: outs.iter().map(|o| o.gauntlet_rejects as f64).sum::<f64>() / n,
        rollbacks_mean: outs.iter().map(|o| o.rollbacks as f64).sum::<f64>() / n,
        cap_hits: outs.iter().filter(|o| o.hit_cap).count(),
    }
}

fn print_table(title: &str, rows: &[Row], show_eff: bool) {
    println!("\n### {title}\n");
    let eff_col = if show_eff { " eff p med (p10-p90) |" } else { "" };
    println!(
        "| policy | final p med (p10-p90) |{eff_col} len med | bug kept | missed | execs med (p90) | accepts | g-rej | rollbacks | cap hits |"
    );
    let eff_dash = if show_eff { " --- |" } else { "" };
    println!("| --- | --- |{eff_dash} --- | --- | --- | --- | --- | --- | --- | --- |");
    for r in rows {
        let eff = if show_eff {
            format!(" {:.2} ({:.2}-{:.2}) |", r.eff_med, r.eff_p10, r.eff_p90)
        } else {
            String::new()
        };
        println!(
            "| {} | {:.2} ({:.2}-{:.2}) |{} {:.0} | {:.0}% | {:.0}% | {:.0} ({:.0}) | {:.1} | {:.1} | {:.2} | {} |",
            r.label,
            r.p_med,
            r.p_p10,
            r.p_p90,
            eff,
            r.len_med,
            r.bug_rate * 100.0,
            r.missed_rate * 100.0,
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        None => run_001(),
        Some("e008-h1") => cmds::cmd_h1(),
        Some("e008-seeding") => cmds::cmd_seeding(),
        Some("e008-factorial") => cmds::cmd_factorial(),
        Some("e008-m") => cmds::cmd_m(),
        Some("e008-recruit") => cmds::cmd_recruit(),
        Some("e008-floor") => cmds::cmd_floor(),
        Some("e008-hw") => cmds::cmd_hw(),
        Some("e008-dp") => cmds::cmd_dp(),
        Some("e008-envelope") => cmds::cmd_envelope(),
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            std::process::exit(2);
        }
    }
}

fn run_001() {
    let fd3 = Stopping::FixedDry(3);
    let policies = [
        Policy::Naive,
        Policy::PerCandidateN { n: 10 },
        Policy::FixedGauntlet { m: 5 },
        Policy::Ledger { gamma: 0.8, checkpoint: false, decay: 1.0 },
        Policy::Ledger { gamma: 0.8, checkpoint: true, decay: 1.0 },
    ];

    println!("# shrink-sim results ({SEEDS} seeds per cell)");
    for landscape in ALL_LANDSCAPES {
        let rows: Vec<Row> = policies
            .iter()
            .map(|&p| summarize(p.name(), &run_trials(landscape, p, fd3)))
            .collect();
        print_table(landscape.name(), &rows, false);
    }

    let gammas = [0.5, 0.8, 1.0];
    for landscape in [Landscape::RisingWithSize, Landscape::NoiseFloor] {
        let rows: Vec<Row> = gammas
            .iter()
            .map(|&gamma| {
                let p = Policy::Ledger { gamma, checkpoint: false, decay: 1.0 };
                summarize(p.name(), &run_trials(landscape, p, fd3))
            })
            .collect();
        print_table(&format!("gamma sensitivity, {}", landscape.name()), &rows, false);
    }

    let stoppings = [
        Stopping::FixedDry(1),
        Stopping::FixedDry(2),
        Stopping::FixedDry(3),
        Stopping::ConfirmedDry,
    ];
    for landscape in [Landscape::RisingWithSize, Landscape::Constant, Landscape::NoiseFloor] {
        let p = Policy::Ledger { gamma: 0.8, checkpoint: false, decay: 1.0 };
        let rows: Vec<Row> = stoppings
            .iter()
            .map(|&s| summarize(format!("{} {}", p.name(), s.name()), &run_trials(landscape, p, s)))
            .collect();
        print_table(&format!("stopping rules, {}", landscape.name()), &rows, false);
    }

    for pin in [Pin::Failing, Pin::Random] {
        let landscape = Landscape::Mixture { pin };
        let mix_policies = [
            Policy::Naive,
            Policy::FixedGauntlet { m: 5 },
            Policy::Ledger { gamma: 0.8, checkpoint: false, decay: 1.0 },
            Policy::Ledger { gamma: 0.8, checkpoint: true, decay: 1.0 },
        ];
        let rows: Vec<Row> = mix_policies
            .iter()
            .map(|&p| summarize(p.name(), &run_trials(landscape, p, fd3)))
            .collect();
        print_table(landscape.name(), &rows, true);
    }

    let decays = [1.0, 0.98, 0.95];
    for landscape in [Landscape::RisingWithSize, Landscape::NoiseFloor] {
        let rows: Vec<Row> = decays
            .iter()
            .map(|&decay| {
                let p = Policy::Ledger { gamma: 0.8, checkpoint: false, decay };
                summarize(p.name(), &run_trials(landscape, p, fd3))
            })
            .collect();
        print_table(&format!("anchor decay, {}", landscape.name()), &rows, false);
    }
}
