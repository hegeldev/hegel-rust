//! Experiment 006A: does the boost phase (successive halving before
//! shrinking) raise incumbent reliability, and at what cost? Spec and
//! results: notes/experiments/006-graft-boost/notes.md

use std::cell::{Cell, RefCell};

use hegel_c::__bench::{nd_boost_experiment, BigInt, Failure, NdShrinkMode, TestCaseResult, ToPrimitive};

const SEEDS: u64 = 30;
const TEST_CASES: u64 = 500;

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
enum Landscape {
    DetCore,
    Rising,
    Constant,
}

const ALL: [Landscape; 3] = [Landscape::DetCore, Landscape::Rising, Landscape::Constant];

impl Landscape {
    fn name(self) -> &'static str {
        match self {
            Landscape::DetCore => "D1 deterministic-core",
            Landscape::Rising => "L1 rising",
            Landscape::Constant => "L3 constant",
        }
    }

    fn p(self, atoms: &[i64]) -> f64 {
        match self {
            Landscape::DetCore => {
                if atoms.iter().any(|&a| a >= 95) {
                    1.0
                } else if atoms.iter().filter(|&&a| a >= 10).count() >= 3 {
                    0.3
                } else {
                    0.0
                }
            }
            Landscape::Rising => {
                if atoms.iter().filter(|&&a| a >= 10).count() >= 3 {
                    (0.1 + 0.08 * (atoms.len().saturating_sub(1)) as f64).clamp(0.1, 0.95)
                } else {
                    0.0
                }
            }
            Landscape::Constant => {
                if atoms.iter().any(|&a| a >= 10) {
                    0.5
                } else {
                    0.0
                }
            }
        }
    }
}

enum Outcome {
    None,
    Shrunk { atoms: Vec<i64>, execs: u64 },
}

fn run_trial(landscape: Landscape, boost: bool, seed: u64) -> Outcome {
    let execs = Cell::new(0u64);
    let hidden = RefCell::new(Rng::new(seed.wrapping_mul(0xC0FFEE) ^ 0xD15EA5E));
    let result = nd_boost_experiment(
        NdShrinkMode::Gauntlet,
        boost,
        seed ^ 0xF00D,
        TEST_CASES,
        std::env::var("ND_DEBUG").is_ok(),
        |ds| {
            execs.set(execs.get() + 1);
            let zero = BigInt::from(0i64);
            let n = match ds.generate_integer(&zero, &BigInt::from(20i64)) {
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
            let result = if hidden.borrow_mut().f64() < p {
                TestCaseResult::Interesting(Failure {
                    origin: "bug".to_string(),
                    reproduce_blob: None,
                })
            } else {
                TestCaseResult::Valid
            };
            ds.mark_complete(&result);
        },
    );
    match result {
        Err(_) => Outcome::None,
        Ok(failures) => match failures.into_iter().next().flatten() {
            None => Outcome::None,
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
    println!("# nd-boost results ({SEEDS} seeds/cell, {TEST_CASES} test-case budget, gauntlet mode)\n");
    println!("| landscape | boost | shrunk | final p med (p10-p90) | det frac | len med | execs med (p90) |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for landscape in ALL {
        for boost in [false, true] {
            let outcomes: Vec<Outcome> = (0..SEEDS)
                .map(|seed| run_trial(landscape, boost, seed))
                .collect();
            let shrunk: Vec<(&Vec<i64>, u64)> = outcomes
                .iter()
                .filter_map(|o| match o {
                    Outcome::Shrunk { atoms, execs } => Some((atoms, *execs)),
                    Outcome::None => None,
                })
                .collect();
            let mut ps: Vec<f64> = shrunk.iter().map(|(a, _)| landscape.p(a)).collect();
            let det = ps.iter().filter(|&&p| p >= 1.0).count();
            let mut lens: Vec<f64> = shrunk.iter().map(|(a, _)| a.len() as f64).collect();
            let mut ex: Vec<f64> = shrunk.iter().map(|(_, e)| *e as f64).collect();
            ps.sort_by(f64::total_cmp);
            lens.sort_by(f64::total_cmp);
            ex.sort_by(f64::total_cmp);
            println!(
                "| {} | {} | {}/{} | {:.2} ({:.2}-{:.2}) | {}/{} | {:.0} | {:.0} ({:.0}) |",
                landscape.name(),
                if boost { "on" } else { "off" },
                shrunk.len(),
                SEEDS,
                percentile(&ps, 0.5),
                percentile(&ps, 0.1),
                percentile(&ps, 0.9),
                det,
                shrunk.len(),
                percentile(&lens, 0.5),
                percentile(&ex, 0.5),
                percentile(&ex, 0.9),
            );
        }
    }
}
