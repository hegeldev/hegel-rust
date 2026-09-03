//! Experiment 008: gauntlet calibration under the shipped rules. Models
//! hegel-c/src/native/nd/mod.rs as of 9c800e8e (Evidence with weighted
//! misses, wilson z=1.96, discovery bar gate 10 / cap 40 / accept 4th fail,
//! gauntlet LCB >= max(gamma*anchor, floor) cap 30, verdict before rerun)
//! plus the remediation-plan factorial. Spec and results:
//! notes/experiments/008-gauntlet-calibration/notes.md

use std::collections::{HashMap, HashSet};

use crate::model::{shortlex_less, Candidate, Landscape, Pin, BUG_THRESHOLD, MIX_W};
use crate::sim::{run_passes, Shrink};
use crate::stats::{percentile, wilson_weighted, Rng};

pub const Z: f64 = 1.96;
pub const GAUNTLET_CAP: u64 = 30;
pub const GATE_RUNS: f64 = 10.0;
pub const CONFIRM_CAP: u64 = 40;
pub const CONFIRM_MIN_FAILS: u64 = 4;
pub const EXEC_CAP: u64 = 200_000;

pub const CHOSEN_MIN_FAILS: u64 = 4;
pub const CHOSEN_FLOOR: f64 = 0.05;
pub const CHOSEN_SEED_RUNS: u64 = 20;
pub const CHOSEN_HIGH_WATER: f64 = 0.8;

#[derive(Clone, Copy, Default, Debug)]
pub struct WEvidence {
    pub fails: u64,
    pub physical: u64,
    pub weighted_misses: f64,
}

impl WEvidence {
    pub fn record(&mut self, failed: bool, weight: f64) {
        self.physical += 1;
        if failed {
            self.fails += 1;
        } else {
            self.weighted_misses += weight;
        }
    }

    fn weighted_total(&self) -> f64 {
        self.fails as f64 + self.weighted_misses
    }

    pub fn lower_bound(&self) -> f64 {
        wilson_weighted(self.fails as f64, self.weighted_total(), Z, false)
    }

    pub fn upper_bound(&self) -> f64 {
        wilson_weighted(self.fails as f64, self.weighted_total(), Z, true)
    }
}

pub enum BarVerdict {
    Accept,
    Reject,
    Continue,
}

pub fn discovery_bar(e: &WEvidence) -> BarVerdict {
    if e.fails >= CONFIRM_MIN_FAILS {
        return BarVerdict::Accept;
    }
    if e.fails == 0 && e.weighted_misses >= GATE_RUNS {
        return BarVerdict::Reject;
    }
    if e.fails + CONFIRM_CAP.saturating_sub(e.physical) < CONFIRM_MIN_FAILS {
        return BarVerdict::Reject;
    }
    BarVerdict::Continue
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Seeding {
    BarBatch,
    Extended(u64),
}

impl Seeding {
    fn name(self) -> String {
        match self {
            Seeding::BarBatch => "bar".to_string(),
            Seeding::Extended(n) => format!("e{n}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AcceptRule {
    pub min_fails: u64,
    pub exclude_recruit: bool,
}

impl AcceptRule {
    fn name(self) -> String {
        match (self.min_fails, self.exclude_recruit) {
            (1, false) => "sh".to_string(),
            (m, false) => format!("m{m}"),
            (m, true) => format!("m{m}x"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gamma {
    Flat(f64),
    HighWater(f64),
}

impl Gamma {
    pub fn value(self, anchor: f64) -> f64 {
        match self {
            Gamma::Flat(g) => g,
            Gamma::HighWater(h) => {
                if anchor >= h {
                    1.0
                } else {
                    0.8
                }
            }
        }
    }

    fn name(self) -> String {
        match self {
            Gamma::Flat(g) => format!("f{g}"),
            Gamma::HighWater(h) => format!("hw{h}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub seeding: Seeding,
    pub rule: AcceptRule,
    pub floor: f64,
    pub gamma: Gamma,
    pub miss_weight: f64,
}

impl Config {
    pub fn name(&self) -> String {
        format!(
            "s={} r={} f={} g={} w={}",
            self.seeding.name(),
            self.rule.name(),
            self.floor,
            self.gamma.name(),
            self.miss_weight
        )
    }

    pub fn shipped() -> Config {
        Config {
            seeding: Seeding::BarBatch,
            rule: AcceptRule { min_fails: 1, exclude_recruit: false },
            floor: 0.05,
            gamma: Gamma::Flat(0.8),
            miss_weight: 1.0,
        }
    }

    pub fn chosen(miss_weight: f64) -> Config {
        Config {
            seeding: Seeding::Extended(CHOSEN_SEED_RUNS),
            rule: AcceptRule { min_fails: CHOSEN_MIN_FAILS, exclude_recruit: false },
            floor: CHOSEN_FLOOR,
            gamma: Gamma::HighWater(CHOSEN_HIGH_WATER),
            miss_weight,
        }
    }
}

pub enum Verdict {
    Accept,
    Reject,
    Continue,
}

pub fn gauntlet_verdict(e: &WEvidence, anchor: f64, cfg: &Config) -> Verdict {
    let threshold = (cfg.gamma.value(anchor) * anchor).max(cfg.floor);
    if e.lower_bound() >= threshold && e.fails >= cfg.rule.min_fails {
        return Verdict::Accept;
    }
    if e.upper_bound() < threshold || e.physical >= GAUNTLET_CAP {
        return Verdict::Reject;
    }
    Verdict::Continue
}

pub struct Outcome008 {
    pub final_len: usize,
    pub final_p: f64,
    pub eff_p: f64,
    pub bug_retained: bool,
    pub deterministic: bool,
    pub seed_anchor: f64,
    pub execs: u64,
    pub accepts: u64,
    pub grej: u64,
    pub bar_rejects: u64,
    pub noise_accepts: u64,
    pub noise_distinct: usize,
    pub hit_cap: bool,
}

pub struct Sim008 {
    landscape: Landscape,
    cfg: Config,
    rng: Rng,
    current: Candidate,
    ledger: HashMap<Candidate, WEvidence>,
    raised: HashSet<Candidate>,
    noise_seen: HashSet<Candidate>,
    pinned_hi: Option<bool>,
    anchor: f64,
    seed_anchor: f64,
    confirm_mode: bool,
    execs: u64,
    accepts: u64,
    grej: u64,
    bar_rejects: u64,
    noise_accepts: u64,
    hit_cap: bool,
}

impl Sim008 {
    pub fn new(landscape: Landscape, cfg: Config, seed: u64, start: Candidate) -> Sim008 {
        let mut sim = Sim008 {
            landscape,
            cfg,
            rng: Rng::new(seed),
            current: start,
            ledger: HashMap::new(),
            raised: HashSet::new(),
            noise_seen: HashSet::new(),
            pinned_hi: None,
            anchor: 0.0,
            seed_anchor: 0.0,
            confirm_mode: false,
            execs: 0,
            accepts: 0,
            grej: 0,
            bar_rejects: 0,
            noise_accepts: 0,
            hit_cap: false,
        };
        sim.confirm_start();
        sim.pin_current();
        sim
    }

    fn exec_fresh(&mut self, c: &Candidate) -> Option<bool> {
        if self.execs >= EXEC_CAP {
            self.hit_cap = true;
            return None;
        }
        self.execs += 1;
        Some(self.rng.f64() < self.landscape.p(c))
    }

    fn confirm_start(&mut self) {
        let start = self.current.clone();
        loop {
            let mut e = WEvidence::default();
            let accepted = loop {
                match discovery_bar(&e) {
                    BarVerdict::Accept => break true,
                    BarVerdict::Reject => break false,
                    BarVerdict::Continue => {}
                }
                let Some(f) = self.exec_fresh(&start) else { return };
                e.record(f, self.cfg.miss_weight);
            };
            if !accepted {
                self.bar_rejects += 1;
                continue;
            }
            if let Seeding::Extended(n) = self.cfg.seeding {
                while e.physical < n {
                    let Some(f) = self.exec_fresh(&start) else { return };
                    e.record(f, self.cfg.miss_weight);
                }
            }
            self.anchor = e.lower_bound();
            self.seed_anchor = self.anchor;
            return;
        }
    }

    fn pin_current(&mut self) {
        self.pinned_hi = match self.landscape {
            Landscape::Mixture { pin } => Some(match pin {
                Pin::Random => self.rng.f64() < MIX_W,
                Pin::Failing => {
                    let hi_mass = MIX_W * crate::model::MIX_P_HI;
                    let total = hi_mass + (1.0 - MIX_W) * crate::model::MIX_P_LO;
                    self.rng.f64() < hi_mass / total
                }
            }),
            _ => None,
        };
    }

    fn accept(&mut self, cand: Candidate) {
        self.accepts += 1;
        if !self.landscape.carries_bug(&cand) {
            self.noise_accepts += 1;
        }
        if let Seeding::Extended(n) = self.cfg.seeding {
            let mut e = self.ledger.get(&cand).copied().unwrap_or_default();
            while e.physical < n {
                let Some(f) = self.exec_fresh(&cand) else { break };
                e.record(f, self.cfg.miss_weight);
            }
            self.ledger.insert(cand.clone(), e);
        }
        let e = self.ledger.get(&cand).copied().unwrap_or_default();
        if self.raised.insert(cand.clone()) {
            let lb = e.lower_bound();
            if lb > self.anchor {
                self.anchor = lb;
            }
        }
        self.current = cand;
        self.pin_current();
    }

    fn sweep(&mut self) -> bool {
        let before = self.accepts;
        run_passes(self);
        self.accepts > before
    }

    fn effective_p(&self) -> f64 {
        match self.pinned_hi {
            Some(hi) => {
                crate::model::REPLAY_FIT * self.landscape.pin_p(&self.current, hi)
                    + (1.0 - crate::model::REPLAY_FIT) * self.landscape.p(&self.current)
            }
            None => self.landscape.p(&self.current),
        }
    }

    pub fn run(&mut self) -> Outcome008 {
        while !self.hit_cap {
            while self.sweep() && !self.hit_cap {}
            if self.hit_cap {
                break;
            }
            self.confirm_mode = true;
            let progressed = self.sweep();
            self.confirm_mode = false;
            if !progressed {
                break;
            }
        }
        let final_p = self.landscape.p(&self.current);
        Outcome008 {
            final_len: self.current.len(),
            final_p,
            eff_p: self.effective_p(),
            bug_retained: self.landscape.carries_bug(&self.current),
            deterministic: final_p == 1.0,
            seed_anchor: self.seed_anchor,
            execs: self.execs,
            accepts: self.accepts,
            grej: self.grej,
            bar_rejects: self.bar_rejects,
            noise_accepts: self.noise_accepts,
            noise_distinct: self.noise_seen.len(),
            hit_cap: self.hit_cap,
        }
    }
}

impl Shrink for Sim008 {
    fn current(&self) -> &Candidate {
        &self.current
    }

    fn consider(&mut self, cand: Candidate) -> bool {
        if self.hit_cap || !shortlex_less(&cand, &self.current) {
            return false;
        }
        if !self.landscape.carries_bug(&cand) {
            self.noise_seen.insert(cand.clone());
        }
        let Some(recruit_fail) = self.exec_fresh(&cand) else {
            return false;
        };
        let mut e = self.ledger.get(&cand).copied().unwrap_or_default();
        if !self.cfg.rule.exclude_recruit {
            e.record(recruit_fail, self.cfg.miss_weight);
            self.ledger.insert(cand.clone(), e);
        }
        if !recruit_fail && !self.confirm_mode {
            return false;
        }
        let accepted = loop {
            match gauntlet_verdict(&e, self.anchor, &self.cfg) {
                Verdict::Accept => break true,
                Verdict::Reject => {
                    self.grej += 1;
                    break false;
                }
                Verdict::Continue => {
                    let Some(f) = self.exec_fresh(&cand) else { break false };
                    e.record(f, self.cfg.miss_weight);
                }
            }
        };
        self.ledger.insert(cand.clone(), e);
        if accepted {
            self.accept(cand);
        }
        accepted
    }
}

pub const START_LEN: usize = 20;
pub const MIN_BUG_ATOMS: usize = 3;

pub fn start_for(landscape: Landscape, rng: &mut Rng, len: usize) -> Candidate {
    loop {
        let c: Candidate = (0..len).map(|_| rng.below(101)).collect();
        if c.iter().filter(|&&a| a >= BUG_THRESHOLD).count() < MIN_BUG_ATOMS.min(len) {
            continue;
        }
        if matches!(landscape, Landscape::BoostCore | Landscape::BoostCoreHi)
            && !c.iter().any(|&a| a >= 95)
        {
            continue;
        }
        return c;
    }
}

pub fn run_trials008(
    landscape: Landscape,
    cfg: Config,
    n: u64,
    start_len: usize,
) -> Vec<Outcome008> {
    (0..n)
        .map(|seed| {
            let mut srng = Rng::new(seed.wrapping_mul(0xA5A5) ^ 0x5EED);
            let start = start_for(landscape, &mut srng, start_len);
            Sim008::new(landscape, cfg, seed ^ 0xF00D, start).run()
        })
        .collect()
}

pub struct Row008 {
    pub label: String,
    pub p50: f64,
    pub p10: f64,
    pub p90: f64,
    pub eff50: f64,
    pub eff10: f64,
    pub eff90: f64,
    pub len50: f64,
    pub bug: f64,
    pub det: f64,
    pub seed_anchor50: f64,
    pub execs50: f64,
    pub execs90: f64,
    pub accepts: f64,
    pub grej: f64,
    pub bar_rejects: f64,
    pub noise_acc: f64,
    pub noise_distinct50: f64,
    pub caps: usize,
}

pub fn summarize008(label: String, outs: &[Outcome008]) -> Row008 {
    fn pct(mut v: Vec<f64>, q: f64) -> f64 {
        v.sort_by(f64::total_cmp);
        percentile(&v, q)
    }
    let n = outs.len() as f64;
    let ps: Vec<f64> = outs.iter().map(|o| o.final_p).collect();
    let effs: Vec<f64> = outs.iter().map(|o| o.eff_p).collect();
    Row008 {
        label,
        p50: pct(ps.clone(), 0.5),
        p10: pct(ps.clone(), 0.1),
        p90: pct(ps, 0.9),
        eff50: pct(effs.clone(), 0.5),
        eff10: pct(effs.clone(), 0.1),
        eff90: pct(effs, 0.9),
        len50: pct(outs.iter().map(|o| o.final_len as f64).collect(), 0.5),
        bug: outs.iter().filter(|o| o.bug_retained).count() as f64 / n,
        det: outs.iter().filter(|o| o.deterministic).count() as f64 / n,
        seed_anchor50: pct(outs.iter().map(|o| o.seed_anchor).collect(), 0.5),
        execs50: pct(outs.iter().map(|o| o.execs as f64).collect(), 0.5),
        execs90: pct(outs.iter().map(|o| o.execs as f64).collect(), 0.9),
        accepts: outs.iter().map(|o| o.accepts as f64).sum::<f64>() / n,
        grej: outs.iter().map(|o| o.grej as f64).sum::<f64>() / n,
        bar_rejects: outs.iter().map(|o| o.bar_rejects as f64).sum::<f64>() / n,
        noise_acc: outs.iter().map(|o| o.noise_accepts as f64).sum::<f64>() / n,
        noise_distinct50: pct(outs.iter().map(|o| o.noise_distinct as f64).collect(), 0.5),
        caps: outs.iter().filter(|o| o.hit_cap).count(),
    }
}

pub fn print_rows008(title: &str, rows: &[Row008], show_eff: bool) {
    println!("\n### {title}\n");
    let eff_col = if show_eff { " eff p med (p10-p90) |" } else { "" };
    println!(
        "| cell | final p med (p10-p90) |{eff_col} len med | bug kept | det | seed anchor med | execs med (p90) | accepts | g-rej | bar-rej | noise acc | noise cands med | cap hits |"
    );
    let eff_dash = if show_eff { " --- |" } else { "" };
    println!(
        "| --- | --- |{eff_dash} --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |"
    );
    for r in rows {
        let eff = if show_eff {
            format!(" {:.2} ({:.2}-{:.2}) |", r.eff50, r.eff10, r.eff90)
        } else {
            String::new()
        };
        println!(
            "| {} | {:.2} ({:.2}-{:.2}) |{} {:.0} | {:.0}% | {:.0}% | {:.3} | {:.0} ({:.0}) | {:.1} | {:.1} | {:.2} | {:.2} | {:.0} | {} |",
            r.label,
            r.p50,
            r.p10,
            r.p90,
            eff,
            r.len50,
            r.bug * 100.0,
            r.det * 100.0,
            r.seed_anchor50,
            r.execs50,
            r.execs90,
            r.accepts,
            r.grej,
            r.bar_rejects,
            r.noise_acc,
            r.noise_distinct50,
            r.caps,
        );
    }
}

pub fn par_map<T: Send, F: Fn(usize) -> T + Send + Sync>(n: usize, f: F) -> Vec<T> {
    let threads = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(4)
        .min(n.max(1));
    let idx = std::sync::atomic::AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        for _ in 0..threads {
            let tx = tx.clone();
            let idx = &idx;
            let f = &f;
            s.spawn(move || loop {
                let i = idx.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= n {
                    break;
                }
                tx.send((i, f(i))).unwrap();
            });
        }
        drop(tx);
    });
    let mut pairs: Vec<(usize, T)> = rx.iter().collect();
    pairs.sort_by_key(|(i, _)| *i);
    pairs.into_iter().map(|(_, t)| t).collect()
}

pub struct SeedSample {
    pub bar: f64,
    pub e20: f64,
    pub e40: f64,
    pub bar_runs: u64,
    pub bar_rejects: u64,
}

pub fn seed_trial(p: f64, miss_weight: f64, rng: &mut Rng) -> SeedSample {
    let mut bar_rejects = 0u64;
    loop {
        let mut e = WEvidence::default();
        let accepted = loop {
            match discovery_bar(&e) {
                BarVerdict::Accept => break true,
                BarVerdict::Reject => break false,
                BarVerdict::Continue => {}
            }
            e.record(rng.f64() < p, miss_weight);
        };
        if !accepted {
            bar_rejects += 1;
            continue;
        }
        let bar = e.lower_bound();
        let bar_runs = e.physical;
        while e.physical < 20 {
            e.record(rng.f64() < p, miss_weight);
        }
        let e20 = e.lower_bound();
        while e.physical < 40 {
            e.record(rng.f64() < p, miss_weight);
        }
        let e40 = e.lower_bound();
        return SeedSample { bar, e20, e40, bar_runs, bar_rejects };
    }
}

pub fn seed_samples(p: f64, miss_weight: f64, n: u64, seed_base: u64) -> Vec<SeedSample> {
    (0..n)
        .map(|i| {
            let mut rng = Rng::new(seed_base.wrapping_add(i).wrapping_mul(0x9E37) ^ 0xBA5E);
            seed_trial(p, miss_weight, &mut rng)
        })
        .collect()
}
