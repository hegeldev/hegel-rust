//! Experiment 003: gauntlet + ledger accepts in the real shrinker, on
//! genuinely flaky bodies. Spec and results: notes/experiments/003-nd-shrink/notes.md

use std::cell::Cell;

use hegel_c::__bench::{nd_shrink_experiment, BigInt, NdShrinkMode, TestCaseResult, ToPrimitive};

const SEEDS: u64 = 100;
const TEST_CASES: u64 = 500;
const BUG_ATOM: i64 = 10;
const MAX_ATOMS: i64 = 20;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Landscape {
    Rising,
    Constant,
    NoiseFloor,
}

impl Landscape {
    fn has_bug(self, atoms: &[i64]) -> bool {
        let bug_atoms = atoms.iter().filter(|&&a| a >= BUG_ATOM).count();
        match self {
            Landscape::Rising => bug_atoms >= 3,
            Landscape::Constant | Landscape::NoiseFloor => bug_atoms >= 1,
        }
    }

    fn p(self, atoms: &[i64]) -> f64 {
        let has_bug = self.has_bug(atoms);
        match self {
            Landscape::Rising => {
                if has_bug {
                    (0.1 + 0.08 * (atoms.len().saturating_sub(1)) as f64).clamp(0.1, 0.95)
                } else {
                    0.0
                }
            }
            Landscape::Constant => {
                if has_bug {
                    0.5
                } else {
                    0.0
                }
            }
            Landscape::NoiseFloor => {
                if has_bug {
                    0.9
                } else {
                    0.02
                }
            }
        }
    }

    fn name(self) -> &'static str {
        match self {
            Landscape::Rising => "L1 rising-with-size",
            Landscape::Constant => "L3 constant p=0.5",
            Landscape::NoiseFloor => "L4 noise-floor",
        }
    }
}

enum Outcome {
    Aborted(String),
    NoBug,
    Shrunk { atoms: Vec<i64>, execs: u64 },
}

fn run_trial(mode: NdShrinkMode, landscape: Landscape, seed: u64) -> Outcome {
    let execs = Cell::new(0u64);
    let hidden = std::cell::RefCell::new(Rng::new(seed.wrapping_mul(0xC0FFEE) ^ 0xD15EA5E));
    let debug = std::env::var("ND_DEBUG").is_ok();
    let result = nd_shrink_experiment(mode, seed ^ 0xF00D, TEST_CASES, debug, |ds| {
        execs.set(execs.get() + 1);
        let zero = BigInt::from(0i64);
        let n = match ds.generate_integer(&zero, &BigInt::from(MAX_ATOMS)) {
            Ok(v) => v.to_i64().unwrap(),
            Err(_) => {
                ds.mark_complete(&TestCaseResult::Overrun);
                return;
            }
        };
        let mut atoms = Vec::with_capacity(n as usize);
        for _ in 0..n {
            match ds.generate_integer(&zero, &BigInt::from(100i64)) {
                Ok(v) => atoms.push(v.to_i64().unwrap()),
                Err(_) => {
                    ds.mark_complete(&TestCaseResult::Overrun);
                    return;
                }
            }
        }
        let p = landscape.p(&atoms);
        if hidden.borrow_mut().f64() < p {
            ds.mark_complete(&TestCaseResult::Interesting(hegel_c::__bench::Failure {
                origin: "bug".to_string(),
                reproduce_blob: None,
            }));
        } else {
            ds.mark_complete(&TestCaseResult::Valid);
        }
    });
    match result {
        Err(msg) => Outcome::Aborted(msg),
        Ok(failures) => match failures.into_iter().next().flatten() {
            None => Outcome::NoBug,
            Some(choices) => Outcome::Shrunk {
                atoms: choices[1..].to_vec(),
                execs: execs.get(),
            },
        },
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn main() {
    if let Ok(spec) = std::env::var("ND_ONE") {
        let parts: Vec<&str> = spec.split(':').collect();
        let mode = match parts[0] {
            "baseline" => NdShrinkMode::Baseline,
            "resample" => NdShrinkMode::Resample,
            _ => NdShrinkMode::Gauntlet,
        };
        let landscape = match parts[1] {
            "l1" => Landscape::Rising,
            "l3" => Landscape::Constant,
            _ => Landscape::NoiseFloor,
        };
        let seed: u64 = parts[2].parse().unwrap();
        match run_trial(mode, landscape, seed) {
            Outcome::Aborted(msg) => println!("aborted: {msg}"),
            Outcome::NoBug => println!("no bug found"),
            Outcome::Shrunk { atoms, execs } => {
                println!("shrunk to {atoms:?} (p={}) in {execs} execs", landscape.p(&atoms));
            }
        }
        return;
    }
    println!("# nd-shrink results ({SEEDS} seeds per cell, {TEST_CASES} test-case budget)\n");
    for landscape in [Landscape::Rising, Landscape::Constant, Landscape::NoiseFloor] {
        println!("### {}\n", landscape.name());
        println!("| mode | shrunk | aborted | no-bug | bug kept | len med | final p med (p10-p90) | execs med (p90) |");
        println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
        for mode in [NdShrinkMode::Baseline, NdShrinkMode::Resample, NdShrinkMode::Gauntlet] {
            let outcomes: Vec<Outcome> = (0..SEEDS)
                .map(|seed| run_trial(mode, landscape, seed))
                .collect();
            let aborted = outcomes.iter().filter(|o| matches!(o, Outcome::Aborted(_))).count();
            let nobug = outcomes.iter().filter(|o| matches!(o, Outcome::NoBug)).count();
            let shrunk: Vec<(&Vec<i64>, u64)> = outcomes
                .iter()
                .filter_map(|o| match o {
                    Outcome::Shrunk { atoms, execs } => Some((atoms, *execs)),
                    _ => None,
                })
                .collect();
            let n = shrunk.len();
            let bug_kept = shrunk
                .iter()
                .filter(|(atoms, _)| landscape.has_bug(atoms))
                .count();
            let mut ps: Vec<f64> = shrunk.iter().map(|(a, _)| landscape.p(a)).collect();
            let mut lens: Vec<f64> = shrunk.iter().map(|(a, _)| a.len() as f64).collect();
            let mut ex: Vec<f64> = shrunk.iter().map(|(_, e)| *e as f64).collect();
            ps.sort_by(f64::total_cmp);
            lens.sort_by(f64::total_cmp);
            ex.sort_by(f64::total_cmp);
            println!(
                "| {:?} | {} | {} | {} | {}/{} | {:.0} | {:.2} ({:.2}-{:.2}) | {:.0} ({:.0}) |",
                mode,
                n,
                aborted,
                nobug,
                bug_kept,
                n,
                percentile(&lens, 0.5),
                percentile(&ps, 0.5),
                percentile(&ps, 0.1),
                percentile(&ps, 0.9),
                percentile(&ex, 0.5),
                percentile(&ex, 0.9),
            );
        }
        println!();
    }
}
