//! Seeded Monte Carlo of bar recycling over a run: one origin, geometric
//! re-sighting, a fresh bar batch per sighting, with and without the
//! per-origin attempt cap. The review path is analytic (main::review_section)
//! and deliberately not composed here: today it is only reachable on the
//! flip-at-final-replay path, which this stream model cannot represent
//! honestly.

use crate::dp::{bar, Verdict};

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Outcome {
    confirmed: bool,
    attempts: u64,
    replays: u64,
}

fn bar_batch(x: f64, rng: &mut Rng) -> (bool, u64) {
    let mut n = 0u64;
    let mut k = 0u64;
    loop {
        match bar(n, k) {
            Verdict::Accept => return (true, n),
            Verdict::Reject => return (false, n),
            Verdict::Continue => {
                n += 1;
                if rng.f64() < x {
                    k += 1;
                }
            }
        }
    }
}

fn episode(x: f64, epochs: u64, cap: Option<u64>, rng: &mut Rng) -> Outcome {
    let sight = (10.0 * x).min(1.0);
    let mut out = Outcome {
        confirmed: false,
        attempts: 0,
        replays: 0,
    };
    for _ in 0..epochs {
        if out.confirmed || cap.is_some_and(|c| out.attempts >= c) {
            break;
        }
        if rng.f64() < sight {
            let (accepted, n) = bar_batch(x, rng);
            out.attempts += 1;
            out.replays += n;
            out.confirmed = accepted;
        }
    }
    out
}

struct MixedOutcome {
    bug: bool,
    fluke: bool,
    attempts: u64,
}

/// The 008 L4b shape: one origin, a bug timeline at p and a fluke timeline
/// at q, each attempt landing on the fluke with probability `share`
/// (record_run re-inserts whichever sighting arrives first after an
/// eviction, so the barred timeline is a mix).
fn mixed_episode(
    p: f64,
    q: f64,
    share: f64,
    epochs: u64,
    cap: Option<u64>,
    rng: &mut Rng,
) -> MixedOutcome {
    let mut out = MixedOutcome {
        bug: false,
        fluke: false,
        attempts: 0,
    };
    for _ in 0..epochs {
        if out.bug || out.fluke || cap.is_some_and(|c| out.attempts >= c) {
            break;
        }
        let x = if rng.f64() < share { q } else { p };
        let (accepted, _) = bar_batch(x, rng);
        out.attempts += 1;
        if accepted {
            if x == q {
                out.fluke = true;
            } else {
                out.bug = true;
            }
        }
    }
    out
}

pub fn mixed_section() {
    println!("\n## Mixed-timeline origin (bug p = 0.1 + fluke q = 0.02, 100k episodes/cell)\n");
    println!(
        "| fluke share | epochs | cap | P(confirm on bug) | P(confirm on fluke) | P(unconfirmed) |"
    );
    println!("| --- | --- | --- | --- | --- | --- |");
    const EPISODES: u64 = 100_000;
    for share in [0.0, 0.25, 0.5, 0.75] {
        for cap in [None, Some(5u64)] {
            let mut rng = Rng::new(0x5eed2027 ^ (share * 1e6) as u64);
            let (mut bug, mut fluke) = (0u64, 0u64);
            for _ in 0..EPISODES {
                let o = mixed_episode(0.1, 0.02, share, 40, cap, &mut rng);
                bug += u64::from(o.bug);
                fluke += u64::from(o.fluke);
            }
            let ep = EPISODES as f64;
            println!(
                "| {share} | 40 | {} | {:.3} | {:.4} | {:.3} |",
                cap.map_or("none".to_string(), |c| c.to_string()),
                bug as f64 / ep,
                fluke as f64 / ep,
                (EPISODES - bug - fluke) as f64 / ep,
            );
        }
    }
}

pub fn run_section() {
    println!("\n## Bar recycling over a run (Monte Carlo, 100k episodes/cell)\n");
    println!("One origin at rate x, sighted per sweep epoch with probability");
    println!("min(1, 10x); each sighting spends one bar batch until confirmation");
    println!("or the cap.\n");
    println!("| x | epochs | cap | P(confirm) | mean attempts | mean replays |");
    println!("| --- | --- | --- | --- | --- | --- |");
    const EPISODES: u64 = 100_000;
    for x in [0.01, 0.02, 0.05, 0.1, 0.3] {
        for epochs in [40u64, 200] {
            for cap in [None, Some(5u64)] {
                let mut rng = Rng::new(0x5eed2026 ^ (x * 1e6) as u64 ^ (epochs << 32));
                let mut confirmed = 0u64;
                let mut attempts = 0u64;
                let mut replays = 0u64;
                for _ in 0..EPISODES {
                    let o = episode(x, epochs, cap, &mut rng);
                    confirmed += u64::from(o.confirmed);
                    attempts += o.attempts;
                    replays += o.replays;
                }
                let ep = EPISODES as f64;
                println!(
                    "| {x} | {epochs} | {} | {:.4} | {:.2} | {:.1} |",
                    cap.map_or("none".to_string(), |c| c.to_string()),
                    confirmed as f64 / ep,
                    attempts as f64 / ep,
                    replays as f64 / ep,
                );
            }
        }
    }
}
