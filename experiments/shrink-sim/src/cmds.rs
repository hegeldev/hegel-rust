//! Experiment 008 subcommands. Each prints one or more of the notes tables;
//! command lines are recorded in notes/experiments/008-gauntlet-calibration/notes.md

use crate::dp::gauntlet_dp;
use crate::e008::{
    par_map, print_rows008, run_trials008, seed_samples, summarize008, AcceptRule, Config, Gamma,
    Row008, Seeding, CHOSEN_FLOOR, CHOSEN_HIGH_WATER, CHOSEN_MIN_FAILS, CHOSEN_SEED_RUNS,
    START_LEN,
};
use crate::model::{Landscape, Pin};
use crate::stats::percentile;

const HEADLINE_N: u64 = 500;
const FACTORIAL_N: u64 = 200;
const SEEDING_N: u64 = 10_000;

const MAIN_LANDSCAPES: [Landscape; 7] = [
    Landscape::RisingWithSize,
    Landscape::DeterministicCore,
    Landscape::Constant,
    Landscape::NoiseFloor,
    Landscape::NoiseFloorLo,
    Landscape::Mixture { pin: Pin::Failing },
    Landscape::BoostCore,
];

fn rules() -> [AcceptRule; 5] {
    [
        AcceptRule { min_fails: 1, exclude_recruit: false },
        AcceptRule { min_fails: 2, exclude_recruit: false },
        AcceptRule { min_fails: 3, exclude_recruit: false },
        AcceptRule { min_fails: 4, exclude_recruit: false },
        AcceptRule { min_fails: 3, exclude_recruit: true },
    ]
}

pub fn all_configs() -> Vec<Config> {
    let mut v = Vec::new();
    for seeding in [Seeding::BarBatch, Seeding::Extended(20), Seeding::Extended(40)] {
        for rule in rules() {
            for floor in [0.035, 0.05, 0.08, 0.10] {
                for gamma in [Gamma::Flat(0.8), Gamma::HighWater(0.7), Gamma::HighWater(0.8)] {
                    for miss_weight in [1.0, 0.2, 0.0] {
                        v.push(Config { seeding, rule, floor, gamma, miss_weight });
                    }
                }
            }
        }
    }
    v
}

fn run_cells(cells: Vec<(String, Landscape, Config, u64, usize)>) -> Vec<Row008> {
    par_map(cells.len(), |i| {
        let (label, landscape, cfg, n, start_len) = cells[i].clone();
        summarize008(label, &run_trials008(landscape, cfg, n, start_len))
    })
}

pub fn cmd_h1() {
    println!("# e008-h1: shipped policy baseline (N={HEADLINE_N} seeds per cell)");
    let shipped = Config::shipped();
    let mut cells = vec![
        ("L1 start len 20 (p .95)".to_string(), Landscape::RisingWithSize, shipped, HEADLINE_N, 20),
        ("L1 start len 10 (p .82)".to_string(), Landscape::RisingWithSize, shipped, HEADLINE_N, 10),
        ("L1 start len 6 (p .50)".to_string(), Landscape::RisingWithSize, shipped, HEADLINE_N, 6),
    ];
    for l in MAIN_LANDSCAPES.iter().skip(1) {
        cells.push((l.name().to_string(), *l, shipped, HEADLINE_N, START_LEN));
    }
    let rows = run_cells(cells);
    print_rows008("shipped policy per landscape", &rows, true);

    println!("\n### DP: shipped rule, P(accept | recruiting run fails) by anchor\n");
    println!("| anchor | threshold | q=0.02 | q=0.1 | q=0.9 | E[runs|fail] q=0.1 |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for anchor in [0.04f64, 0.10, 0.2065, 0.25, 0.258, 0.30, 0.3755, 0.51, 0.839] {
        let th = (0.8 * anchor).max(shipped.floor);
        let r: Vec<_> = [0.02, 0.1, 0.9]
            .iter()
            .map(|&q| gauntlet_dp(q, anchor, &shipped))
            .collect();
        println!(
            "| {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.1} |",
            anchor, th, r[0].accept_given_fail, r[1].accept_given_fail, r[2].accept_given_fail,
            r[1].runs_given_fail,
        );
    }
}

pub fn cmd_seeding() {
    println!("# e008-seeding: anchor seeding study (N={SEEDING_N} batches per cell)");
    println!("\n### seeded anchor by true p, batch size, miss weight\n");
    println!("| p | w | bar med (p10-p90) | bar runs med | bar rejects/accept | e20 med | e40 med | |bar-e40| | |e20-e40| |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for &p in &[0.1, 0.3, 0.5, 0.7, 0.9, 1.0] {
        for &w in &[1.0, 0.2, 0.0] {
            let samples = seed_samples(p, w, SEEDING_N, (p * 1000.0) as u64 + (w * 100.0) as u64);
            let mut bar: Vec<f64> = samples.iter().map(|s| s.bar).collect();
            let mut e20: Vec<f64> = samples.iter().map(|s| s.e20).collect();
            let mut e40: Vec<f64> = samples.iter().map(|s| s.e40).collect();
            let mut runs: Vec<f64> = samples.iter().map(|s| s.bar_runs as f64).collect();
            bar.sort_by(f64::total_cmp);
            e20.sort_by(f64::total_cmp);
            e40.sort_by(f64::total_cmp);
            runs.sort_by(f64::total_cmp);
            let rej: f64 = samples.iter().map(|s| s.bar_rejects as f64).sum::<f64>()
                / samples.len() as f64;
            let (bm, em20, em40) =
                (percentile(&bar, 0.5), percentile(&e20, 0.5), percentile(&e40, 0.5));
            println!(
                "| {:.1} | {:.1} | {:.3} ({:.3}-{:.3}) | {:.0} | {:.2} | {:.3} | {:.3} | {:.3} | {:.3} |",
                p, w, bm,
                percentile(&bar, 0.1),
                percentile(&bar, 0.9),
                percentile(&runs, 0.5),
                rej, em20, em40,
                (bm - em40).abs(),
                (em20 - em40).abs(),
            );
        }
    }

    println!("\n### boost floor: trigger precision/recall over extended-20 anchors (w=1.0)\n");
    let pops: [(&str, Vec<f64>); 2] = [
        ("{0.1,0.3,0.9}", vec![0.1, 0.3, 0.9]),
        ("{0.1,0.3,0.5,0.7,0.9}", vec![0.1, 0.3, 0.5, 0.7, 0.9]),
    ];
    for (pop_name, pop) in pops {
        println!("\npopulation {pop_name} (target class: true p < 0.5)\n");
        println!("| floor | trigger rate per p | precision | recall |");
        println!("| --- | --- | --- | --- |");
        let per_p: Vec<(f64, Vec<f64>)> = pop
            .iter()
            .map(|&p| {
                let s = seed_samples(p, 1.0, SEEDING_N, (p * 1000.0) as u64 + 100);
                (p, s.iter().map(|x| x.e20).collect())
            })
            .collect();
        for floor in [0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50] {
            let mut trig_target = 0.0;
            let mut trig_other = 0.0;
            let mut n_target = 0.0;
            let mut detail = String::new();
            for (p, anchors) in &per_p {
                let rate =
                    anchors.iter().filter(|&&a| a < floor).count() as f64 / anchors.len() as f64;
                detail.push_str(&format!("p{p}:{rate:.3} "));
                if *p < 0.5 {
                    trig_target += rate;
                    n_target += 1.0;
                } else {
                    trig_other += rate;
                }
            }
            let precision = if trig_target + trig_other > 0.0 {
                trig_target / (trig_target + trig_other)
            } else {
                f64::NAN
            };
            println!(
                "| {:.2} | {} | {:.3} | {:.3} |",
                floor,
                detail.trim_end(),
                precision,
                trig_target / n_target,
            );
        }
    }
}

pub fn cmd_factorial() {
    println!("# e008-factorial: full factorial, N={FACTORIAL_N} seeds per cell");
    println!("\n| cfg | L1 p50 | L1 len | L1 ex50 | L2 p50 | L3 p50 | L4 bug | L4b bug | L4b p50 | L5 eff50 | D1 det | D1 ex50 | caps |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    let configs = all_configs();
    let lines = par_map(configs.len(), |i| {
        let cfg = configs[i];
        let rows: Vec<Row008> = MAIN_LANDSCAPES
            .iter()
            .map(|&l| summarize008(l.name().to_string(), &run_trials008(l, cfg, FACTORIAL_N, START_LEN)))
            .collect();
        let caps: usize = rows.iter().map(|r| r.caps).sum();
        format!(
            "| {} | {:.2} | {:.0} | {:.0} | {:.2} | {:.2} | {:.0}% | {:.0}% | {:.2} | {:.2} | {:.0}% | {:.0} | {} |",
            cfg.name(),
            rows[0].p50, rows[0].len50, rows[0].execs50,
            rows[1].p50, rows[2].p50,
            rows[3].bug * 100.0,
            rows[4].bug * 100.0, rows[4].p50,
            rows[5].eff50,
            rows[6].det * 100.0, rows[6].execs50,
            caps,
        )
    });
    for l in lines {
        println!("{l}");
    }
}

pub fn cmd_m() {
    println!("# e008-m: min-fails sweep at s=e20 f=0.05 g=f0.8 w=1 (N={HEADLINE_N})");
    for landscape in [
        Landscape::RisingWithSize,
        Landscape::Constant,
        Landscape::NoiseFloor,
        Landscape::NoiseFloorLo,
        Landscape::BoostCore,
    ] {
        let cells: Vec<_> = rules()
            .iter()
            .map(|&rule| {
                let cfg = Config {
                    seeding: Seeding::Extended(20),
                    rule,
                    floor: 0.05,
                    gamma: Gamma::Flat(0.8),
                    miss_weight: 1.0,
                };
                (cfg.name(), landscape, cfg, HEADLINE_N, START_LEN)
            })
            .collect();
        let rows = run_cells(cells);
        print_rows008(landscape.name(), &rows, false);
        let base = rows[0].execs50;
        let ratios: Vec<String> = rows
            .iter()
            .map(|r| format!("{}: {:.2}", r.label, r.execs50 / base))
            .collect();
        println!("\ncost ratio vs sh (execs med): {}", ratios.join(", "));
    }
}

pub fn cmd_recruit() {
    println!("# e008-recruit: recruit inclusion vs exclusion at min-fails 3");
    println!("\n### DP per-proposal accept (floor 0.05, flat 0.8, w=1.0)\n");
    println!("| anchor | q | m3 P(acc|fail) | m3x P(acc|fail) | ratio | m3 E[runs|fail] | m3x E[runs|fail] |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    let m3 = Config {
        seeding: Seeding::Extended(20),
        rule: AcceptRule { min_fails: 3, exclude_recruit: false },
        floor: 0.05,
        gamma: Gamma::Flat(0.8),
        miss_weight: 1.0,
    };
    let m3x = Config { rule: AcceptRule { min_fails: 3, exclude_recruit: true }, ..m3 };
    for anchor in [0.0, 0.05, 0.30, 0.839] {
        for q in [0.02, 0.1, 0.9] {
            let a = gauntlet_dp(q, anchor, &m3);
            let b = gauntlet_dp(q, anchor, &m3x);
            println!(
                "| {:.3} | {:.2} | {:.5} | {:.5} | {:.2} | {:.1} | {:.1} |",
                anchor, q,
                a.accept_given_fail,
                b.accept_given_fail,
                b.accept_given_fail / a.accept_given_fail.max(1e-12),
                a.runs_given_fail,
                b.runs_given_fail,
            );
        }
    }
    println!("\n### sim: m3 vs m3x (s=e20 f=0.05 g=f0.8 w=1, N={HEADLINE_N})\n");
    let mut cells = Vec::new();
    for landscape in [Landscape::RisingWithSize, Landscape::NoiseFloorLo] {
        for cfg in [m3, m3x] {
            cells.push((
                format!("{} {}", landscape.name(), cfg.name()),
                landscape,
                cfg,
                HEADLINE_N,
                START_LEN,
            ));
        }
    }
    let rows = run_cells(cells);
    print_rows008("m3 vs m3x", &rows, false);
}

pub fn cmd_floor() {
    println!("# e008-floor: floor derivation at an uninformative anchor (threshold = floor)");
    println!("\n### DP at anchor 0, w=1.0, flat 0.8: per-proposal operating points\n");
    println!("| m | floor | alpha cond (q=.02) | alpha uncond | per-shrink K=30 | per-shrink K=60 | power (q=.1) | power/ceiling | E[runs|fail] q=.1 |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for m in [1u64, 2, 3, 4] {
        let ceiling = {
            let cfg = Config {
                seeding: Seeding::Extended(20),
                rule: AcceptRule { min_fails: m, exclude_recruit: false },
                floor: 0.0,
                gamma: Gamma::Flat(0.8),
                miss_weight: 1.0,
            };
            gauntlet_dp(0.1, 0.0, &cfg).accept_given_fail
        };
        for floor in [0.0, 0.035, 0.05, 0.08, 0.10] {
            let cfg = Config {
                seeding: Seeding::Extended(20),
                rule: AcceptRule { min_fails: m, exclude_recruit: false },
                floor,
                gamma: Gamma::Flat(0.8),
                miss_weight: 1.0,
            };
            let noise = gauntlet_dp(0.02, 0.0, &cfg);
            let power = gauntlet_dp(0.1, 0.0, &cfg);
            println!(
                "| {} | {:.3} | {:.5} | {:.5} | {:.4} | {:.4} | {:.4} | {:.3} | {:.1} |",
                m, floor,
                noise.accept_given_fail,
                noise.accept_unconditional,
                1.0 - (1.0 - noise.accept_unconditional).powi(30),
                1.0 - (1.0 - noise.accept_unconditional).powi(60),
                power.accept_given_fail,
                power.accept_given_fail / ceiling,
                power.runs_given_fail,
            );
        }
    }
    println!("\n### sim: L4b noise-floor-lo (bug p=0.1, noise p=0.02), s=e20 g=f0.8 w=1, N={HEADLINE_N}\n");
    let mut cells = Vec::new();
    for m in [2u64, 3, 4] {
        for floor in [0.035, 0.05, 0.08, 0.10] {
            let cfg = Config {
                seeding: Seeding::Extended(20),
                rule: AcceptRule { min_fails: m, exclude_recruit: false },
                floor,
                gamma: Gamma::Flat(0.8),
                miss_weight: 1.0,
            };
            cells.push((cfg.name(), Landscape::NoiseFloorLo, cfg, HEADLINE_N, START_LEN));
        }
    }
    let rows = run_cells(cells);
    print_rows008("L4b floor sweep", &rows, false);
}

pub fn cmd_hw() {
    println!(
        "# e008-hw: retention high-water sweep at r=m{CHOSEN_MIN_FAILS} f={CHOSEN_FLOOR} w=1 (N={HEADLINE_N})"
    );
    let mut cells = Vec::new();
    for landscape in [
        Landscape::BoostCore,
        Landscape::BoostCoreHi,
        Landscape::DeterministicCore,
        Landscape::RisingWithSize,
        Landscape::Constant,
    ] {
        for gamma in [Gamma::Flat(0.8), Gamma::HighWater(0.7), Gamma::HighWater(0.8)] {
            let cfg = Config {
                seeding: Seeding::Extended(20),
                rule: AcceptRule { min_fails: CHOSEN_MIN_FAILS, exclude_recruit: false },
                floor: CHOSEN_FLOOR,
                gamma,
                miss_weight: 1.0,
            };
            cells.push((
                format!("{} {}", landscape.name(), cfg.name()),
                landscape,
                cfg,
                HEADLINE_N,
                START_LEN,
            ));
        }
    }
    for gamma in [Gamma::HighWater(0.7), Gamma::HighWater(0.8)] {
        let cfg = Config {
            seeding: Seeding::Extended(40),
            rule: AcceptRule { min_fails: CHOSEN_MIN_FAILS, exclude_recruit: false },
            floor: CHOSEN_FLOOR,
            gamma,
            miss_weight: 1.0,
        };
        cells.push((
            format!("{} {}", Landscape::BoostCore.name(), cfg.name()),
            Landscape::BoostCore,
            cfg,
            HEADLINE_N,
            START_LEN,
        ));
    }
    let rows = run_cells(cells);
    print_rows008("high-water sweep", &rows, false);
}

pub fn cmd_dp() {
    println!("# e008-dp: per-candidate operating points (005A method, z=1.96)");
    println!("\n| cfg | w | anchor | threshold | q | P(acc|fail) | P(acc) uncond | E[runs|fail] |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    let shipped = Config::shipped();
    let mut cfgs = vec![("shipped".to_string(), shipped)];
    for w in [1.0, 0.2, 0.0] {
        cfgs.push((format!("chosen w={w}"), Config::chosen(w)));
    }
    for (name, cfg) in cfgs {
        for anchor in [0.05, 0.30, 0.839] {
            let th = (cfg.gamma.value(anchor) * anchor).max(cfg.floor);
            for q in [0.02, 0.1, 0.9] {
                let r = gauntlet_dp(q, anchor, &cfg);
                println!(
                    "| {} | {} | {:.3} | {:.4} | {:.2} | {:.5} | {:.6} | {:.1} |",
                    name, cfg.miss_weight, anchor, th, q,
                    r.accept_given_fail, r.accept_unconditional, r.runs_given_fail,
                );
            }
        }
    }
}

pub fn cmd_envelope() {
    println!(
        "# e008-envelope: drift envelope under the chosen rule (s=e{CHOSEN_SEED_RUNS} r=m{CHOSEN_MIN_FAILS} f={CHOSEN_FLOOR} g=hw{CHOSEN_HIGH_WATER}), N={HEADLINE_N}"
    );
    for w in [1.0, 0.2, 0.0] {
        let cfg = Config::chosen(w);
        let cells: Vec<_> = MAIN_LANDSCAPES
            .iter()
            .chain([Landscape::BoostCoreHi].iter())
            .map(|&l| (l.name().to_string(), l, cfg, HEADLINE_N, START_LEN))
            .collect();
        let rows = run_cells(cells);
        print_rows008(&format!("miss weight {w}"), &rows, true);
    }
}
