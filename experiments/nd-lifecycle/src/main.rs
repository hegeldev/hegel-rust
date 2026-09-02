//! Experiment 005B: the ND failure lifecycle end to end — discover, confirm
//! (gate 1/10 then 4/40), capture, shrink, persist, then reproduce from the
//! database in a second run. Spec and results:
//! notes/experiments/005-lifecycle/notes.md

use std::cell::{Cell, RefCell};

use hegel_c::__bench::{
    nd_lifecycle_experiment, BigInt, DataSource, Failure, NdShrinkMode, TestCaseResult,
    ToPrimitive,
};

const SEEDS: u64 = 30;

fn test_cases() -> u64 {
    std::env::var("ND_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300)
}

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
    Rising,
    Constant,
    NoiseFloor,
    StepCoins,
    HetShift,
    NoiseOnly,
}

const ALL_BODIES: [Body; 6] = [
    Body::Rising,
    Body::Constant,
    Body::NoiseFloor,
    Body::StepCoins,
    Body::HetShift,
    Body::NoiseOnly,
];

impl Body {
    fn name(self) -> &'static str {
        match self {
            Body::Rising => "L1 rising",
            Body::Constant => "L3 constant",
            Body::NoiseFloor => "L4 noise-floor",
            Body::StepCoins => "S2 step-coins",
            Body::HetShift => "S5 het-shift",
            Body::NoiseOnly => "N0 noise-only",
        }
    }
}

fn int(ds: &dyn DataSource, lo: i64, hi: i64) -> Result<i64, ()> {
    ds.generate_integer(&BigInt::from(lo), &BigInt::from(hi))
        .map(|v| v.to_i64().unwrap())
        .map_err(|_| ())
}

fn draws(body: Body, hidden: &mut Rng, ds: &dyn DataSource) -> Result<Vec<i64>, ()> {
    let mut atoms = Vec::new();
    match body {
        Body::Rising | Body::Constant | Body::NoiseFloor | Body::NoiseOnly => {
            for _ in 0..int(ds, 0, 20)? {
                atoms.push(int(ds, 0, 100)?);
            }
        }
        Body::StepCoins => {
            for _ in 0..int(ds, 0, 10)? {
                atoms.push(int(ds, 0, 100)?);
                if hidden.f64() < 0.2 {
                    atoms.push(int(ds, 0, 100)?);
                }
            }
        }
        Body::HetShift => {
            for _ in 0..int(ds, 0, 10)? {
                atoms.push(int(ds, 0, 100)?);
                int(ds, 0, 1)?;
                if hidden.f64() < 0.2 {
                    int(ds, 0, 1)?;
                }
            }
        }
    }
    Ok(atoms)
}

fn fails(body: Body, atoms: &[i64], hidden: &mut Rng) -> bool {
    match body {
        Body::Rising => {
            let bug = atoms.iter().filter(|&&a| a >= 10).count() >= 3;
            let p = if bug {
                (0.1 + 0.08 * (atoms.len().saturating_sub(1)) as f64).clamp(0.1, 0.95)
            } else {
                0.0
            };
            hidden.f64() < p
        }
        Body::Constant => {
            let bug = atoms.iter().any(|&a| a >= 10);
            bug && hidden.f64() < 0.5
        }
        Body::NoiseFloor => {
            let bug = atoms.iter().any(|&a| a >= 10);
            let p = if bug { 0.9 } else { 0.02 };
            hidden.f64() < p
        }
        Body::NoiseOnly => hidden.f64() < 0.02,
        Body::StepCoins | Body::HetShift => atoms.iter().filter(|&&a| a >= 90).count() >= 3,
    }
}

struct RunStats {
    confirmed: bool,
    caveated: bool,
    execs: u64,
    err: Option<String>,
}

fn one_run(body: Body, engine_seed: u64, hidden_seed: u64, db_path: &str) -> RunStats {
    let execs = Cell::new(0u64);
    let hidden = RefCell::new(Rng::new(hidden_seed));
    let result = nd_lifecycle_experiment(
        NdShrinkMode::Gauntlet,
        engine_seed,
        test_cases(),
        db_path,
        body.name(),
        std::env::var("ND_DEBUG").is_ok(),
        |ds| {
            execs.set(execs.get() + 1);
            let drawn = draws(body, &mut hidden.borrow_mut(), ds.as_ref());
            match drawn {
                Err(()) => ds.mark_complete(&TestCaseResult::Overrun),
                Ok(atoms) => {
                    let result = if fails(body, &atoms, &mut hidden.borrow_mut()) {
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
        },
    );
    match result {
        Err(e) => RunStats {
            confirmed: false,
            caveated: false,
            execs: execs.get(),
            err: Some(e),
        },
        Ok(failures) => RunStats {
            confirmed: failures.iter().any(|(o, _)| o == "bug"),
            caveated: failures.iter().any(|(o, _)| o.starts_with("[unconfirmed")),
            execs: execs.get(),
            err: None,
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
        let body = *ALL_BODIES
            .iter()
            .find(|b| b.name().starts_with(parts[0]))
            .unwrap();
        let seed: u64 = parts[1].parse().unwrap();
        let dir = std::env::temp_dir().join(format!("nd-lifecycle-one-{}", std::process::id()));
        let db_path = dir.to_str().unwrap().to_string();
        let r1 = one_run(body, seed ^ 0xF00D, seed * 2, &db_path);
        println!(
            "run1: confirmed={} caveated={} execs={} err={:?}",
            r1.confirmed, r1.caveated, r1.execs, r1.err
        );
        let r2 = one_run(body, seed ^ 0xBEEF, seed * 2 + 1, &db_path);
        println!(
            "run2: confirmed={} caveated={} execs={} err={:?}",
            r2.confirmed, r2.caveated, r2.execs, r2.err
        );
        std::fs::remove_dir_all(&dir).ok();
        return;
    }
    println!(
        "# nd-lifecycle results ({SEEDS} seeds/body, {} test-case budget, gauntlet mode)\n",
        test_cases()
    );
    println!("| body | r1 confirmed | r1 caveated | r1 execs med | r2 reproduced | r2 caveated | r2 neither | r2 execs med | errors |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    let base = std::env::temp_dir().join(format!("nd-lifecycle-{}", std::process::id()));
    for body in ALL_BODIES {
        let mut r1_confirmed = 0;
        let mut r1_caveated = 0;
        let mut r2_reproduced = 0;
        let mut r2_caveated = 0;
        let mut r2_neither = 0;
        let mut errors = 0;
        let mut e1: Vec<f64> = Vec::new();
        let mut e2: Vec<f64> = Vec::new();
        for seed in 0..SEEDS {
            let dir = base.join(format!("{}-{seed}", body.name().replace(' ', "-")));
            let db_path = dir.to_str().unwrap().to_string();
            let r1 = one_run(body, seed ^ 0xF00D, seed * 2, &db_path);
            let r2 = one_run(body, seed ^ 0xBEEF, seed * 2 + 1, &db_path);
            std::fs::remove_dir_all(&dir).ok();
            errors += usize::from(r1.err.is_some()) + usize::from(r2.err.is_some());
            r1_confirmed += usize::from(r1.confirmed);
            r1_caveated += usize::from(r1.caveated);
            if r1.confirmed {
                r2_reproduced += usize::from(r2.confirmed);
                r2_caveated += usize::from(r2.caveated);
                r2_neither += usize::from(!r2.confirmed && !r2.caveated);
                e2.push(r2.execs as f64);
            }
            e1.push(r1.execs as f64);
        }
        e1.sort_by(f64::total_cmp);
        e2.sort_by(f64::total_cmp);
        println!(
            "| {} | {}/{} | {} | {:.0} | {}/{} | {} | {} | {:.0} | {} |",
            body.name(),
            r1_confirmed,
            SEEDS,
            r1_caveated,
            percentile(&e1, 0.5),
            r2_reproduced,
            r1_confirmed,
            r2_caveated,
            r2_neither,
            percentile(&e2, 0.5),
            errors,
        );
    }
    std::fs::remove_dir_all(&base).ok();
}
