use std::collections::{HashMap, HashSet};

use crate::model::{has_bug, shortlex_less, Candidate, Landscape, Pin, MIX_P_HI, MIX_P_LO, MIX_W, REPLAY_FIT};
use crate::stats::{wilson_lcb, wilson_ucb, Evidence, Rng};

const Z: f64 = 1.96;
const GAUNTLET_CAP: u64 = 30;
const CONFIRM_RUNS: u64 = 20;
const VALIDATE_STEP: u64 = 10;
const VALIDATE_CAP: u64 = 40;
const EXEC_CAP: u64 = 200_000;
const THRESHOLD_FLOOR: f64 = 0.05;

#[derive(Clone, Copy, Debug)]
pub enum Policy {
    Naive,
    PerCandidateN { n: u64 },
    FixedGauntlet { m: u64 },
    Ledger { gamma: f64, checkpoint: bool, decay: f64 },
}

impl Policy {
    pub fn name(self) -> String {
        match self {
            Policy::Naive => "P0 naive".to_string(),
            Policy::PerCandidateN { n } => format!("P1 fail-within-{n}"),
            Policy::FixedGauntlet { m } => format!("P2 gauntlet-m{m}"),
            Policy::Ledger { gamma, checkpoint, decay } => {
                let base = if checkpoint {
                    format!("P4 ledger+ckpt g{gamma}")
                } else {
                    format!("P3 ledger g{gamma}")
                };
                if decay < 1.0 {
                    format!("{base} d{decay}")
                } else {
                    base
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Stopping {
    FixedDry(u32),
    ConfirmedDry,
}

impl Stopping {
    pub fn name(self) -> String {
        match self {
            Stopping::FixedDry(k) => format!("dry-{k}"),
            Stopping::ConfirmedDry => "confirmed".to_string(),
        }
    }
}

pub struct Outcome {
    pub final_len: usize,
    pub final_p: f64,
    pub eff_p: f64,
    pub bug_retained: bool,
    pub missed: bool,
    pub execs: u64,
    pub accepts: u64,
    pub gauntlet_rejects: u64,
    pub rollbacks: u64,
    pub hit_cap: bool,
}

pub struct Sim {
    landscape: Landscape,
    policy: Policy,
    stopping: Stopping,
    rng: Rng,
    current: Candidate,
    ledger: HashMap<Candidate, Evidence>,
    poisoned: HashSet<Candidate>,
    accepted_since_snapshot: Vec<Candidate>,
    snapshot: Candidate,
    snapshot_pin: Option<bool>,
    pinned_hi: Option<bool>,
    incumbent_evidence: Evidence,
    confirm_mode: bool,
    anchor: f64,
    execs: u64,
    accepts: u64,
    gauntlet_rejects: u64,
    rollbacks: u64,
    hit_cap: bool,
}

impl Sim {
    pub fn new(
        landscape: Landscape,
        policy: Policy,
        stopping: Stopping,
        seed: u64,
        start: Candidate,
    ) -> Sim {
        let mut sim = Sim {
            landscape,
            policy,
            stopping,
            rng: Rng::new(seed),
            snapshot: start.clone(),
            snapshot_pin: None,
            current: start,
            ledger: HashMap::new(),
            poisoned: HashSet::new(),
            accepted_since_snapshot: Vec::new(),
            pinned_hi: None,
            incumbent_evidence: Evidence::default(),
            confirm_mode: false,
            anchor: 0.0,
            execs: 0,
            accepts: 0,
            gauntlet_rejects: 0,
            rollbacks: 0,
            hit_cap: false,
        };
        if let Policy::Ledger { .. } = sim.policy {
            let start = sim.current.clone();
            for _ in 0..CONFIRM_RUNS {
                sim.execute(&start);
            }
            let e = sim.evidence(&start);
            sim.anchor = wilson_lcb(e.fails, e.runs, Z);
        }
        sim.pin_current();
        sim.snapshot_pin = sim.pinned_hi;
        sim
    }

    pub fn current(&self) -> &Candidate {
        &self.current
    }

    fn pin_current(&mut self) {
        self.pinned_hi = match self.landscape {
            Landscape::Mixture { pin } => Some(match pin {
                Pin::Random => self.rng.f64() < MIX_W,
                Pin::Failing => {
                    let hi_mass = MIX_W * MIX_P_HI;
                    let total = hi_mass + (1.0 - MIX_W) * MIX_P_LO;
                    self.rng.f64() < hi_mass / total
                }
            }),
            _ => None,
        };
        self.incumbent_evidence = Evidence::default();
    }

    fn record(&mut self, c: &Candidate, fail: bool) {
        let e = self.ledger.entry(c.clone()).or_default();
        e.runs += 1;
        if fail {
            e.fails += 1;
        }
    }

    fn execute(&mut self, c: &Candidate) -> bool {
        if self.execs >= EXEC_CAP {
            self.hit_cap = true;
            return false;
        }
        self.execs += 1;
        let fail = self.rng.f64() < self.landscape.p(c);
        self.record(c, fail);
        fail
    }

    fn execute_incumbent(&mut self) -> bool {
        if self.execs >= EXEC_CAP {
            self.hit_cap = true;
            return false;
        }
        self.execs += 1;
        let c = self.current.clone();
        let p = match self.pinned_hi {
            Some(hi) if self.rng.f64() < REPLAY_FIT => self.landscape.pin_p(&c, hi),
            _ => self.landscape.p(&c),
        };
        let fail = self.rng.f64() < p;
        self.record(&c, fail);
        self.incumbent_evidence.runs += 1;
        if fail {
            self.incumbent_evidence.fails += 1;
        }
        fail
    }

    fn evidence(&self, c: &Candidate) -> Evidence {
        self.ledger.get(c).copied().unwrap_or_default()
    }

    fn threshold(&self) -> f64 {
        match self.policy {
            Policy::Ledger { gamma, .. } => (gamma * self.anchor).max(THRESHOLD_FLOOR),
            _ => THRESHOLD_FLOOR,
        }
    }

    pub fn consider(&mut self, cand: Candidate) -> bool {
        if self.hit_cap || !shortlex_less(&cand, &self.current) || self.poisoned.contains(&cand) {
            return false;
        }
        let accept = match self.policy {
            Policy::Naive => self.execute(&cand),
            Policy::PerCandidateN { n } => {
                let mut ok = false;
                for _ in 0..n {
                    if self.execute(&cand) {
                        ok = true;
                        break;
                    }
                    if self.hit_cap {
                        break;
                    }
                }
                ok
            }
            Policy::FixedGauntlet { m } => {
                if !self.execute(&cand) {
                    false
                } else {
                    let mut ok = true;
                    for _ in 1..m {
                        if !self.execute(&cand) {
                            ok = false;
                            self.gauntlet_rejects += 1;
                            break;
                        }
                    }
                    ok
                }
            }
            Policy::Ledger { .. } => {
                if !self.confirm_mode && !self.execute(&cand) {
                    false
                } else {
                    let threshold = self.threshold();
                    loop {
                        let e = self.evidence(&cand);
                        if wilson_lcb(e.fails, e.runs, Z) >= threshold {
                            break true;
                        }
                        if wilson_ucb(e.fails, e.runs, Z) < threshold || e.runs >= GAUNTLET_CAP {
                            self.gauntlet_rejects += 1;
                            break false;
                        }
                        if self.hit_cap {
                            break false;
                        }
                        self.execute(&cand);
                    }
                }
            }
        };
        if accept {
            self.accept(cand);
        }
        accept
    }

    fn accept(&mut self, cand: Candidate) {
        self.accepts += 1;
        self.accepted_since_snapshot.push(cand.clone());
        self.current = cand;
        if let Policy::Ledger { .. } = self.policy {
            let e = self.evidence(&self.current);
            let lcb = wilson_lcb(e.fails, e.runs, Z);
            if lcb > self.anchor {
                self.anchor = lcb;
            }
        }
        self.pin_current();
    }

    fn checkpoint(&mut self) {
        if !matches!(self.policy, Policy::Ledger { checkpoint: true, .. }) {
            return;
        }
        let target = (self.incumbent_evidence.runs + VALIDATE_STEP).min(VALIDATE_CAP);
        while self.incumbent_evidence.runs < target && !self.hit_cap {
            self.execute_incumbent();
        }
        let e = self.incumbent_evidence;
        let bar = 0.5 * self.anchor;
        if wilson_ucb(e.fails, e.runs, Z) < bar {
            self.rollbacks += 1;
            for c in self.accepted_since_snapshot.drain(..) {
                self.poisoned.insert(c);
            }
            self.current = self.snapshot.clone();
            self.pinned_hi = self.snapshot_pin;
            self.incumbent_evidence = Evidence::default();
        } else if wilson_lcb(e.fails, e.runs, Z) >= bar {
            self.snapshot = self.current.clone();
            self.snapshot_pin = self.pinned_hi;
            self.accepted_since_snapshot.clear();
        }
    }

    fn sweep(&mut self) -> bool {
        let before = self.accepts;
        run_passes(self);
        self.checkpoint();
        if let Policy::Ledger { decay, .. } = self.policy {
            if decay < 1.0 {
                self.anchor *= decay;
            }
        }
        self.accepts > before
    }

    fn effective_p(&self) -> f64 {
        match self.pinned_hi {
            Some(hi) => {
                REPLAY_FIT * self.landscape.pin_p(&self.current, hi)
                    + (1.0 - REPLAY_FIT) * self.landscape.p(&self.current)
            }
            None => self.landscape.p(&self.current),
        }
    }

    fn oracle_missed(&self) -> bool {
        if !matches!(self.policy, Policy::Ledger { .. }) {
            return false;
        }
        let threshold = self.threshold();
        let cur = &self.current;
        for k in [8usize, 4, 2, 1] {
            if cur.len() < k {
                continue;
            }
            for i in 0..=cur.len() - k {
                let mut cand = cur.clone();
                cand.drain(i..i + k);
                if self.landscape.p(&cand) >= threshold {
                    return true;
                }
            }
        }
        for i in 0..cur.len() {
            let v = cur[i];
            if v == 0 {
                continue;
            }
            let mut vals = vec![0, v - 1];
            if v > crate::model::BUG_THRESHOLD {
                vals.push(crate::model::BUG_THRESHOLD);
            }
            for val in vals {
                let mut cand = cur.clone();
                cand[i] = val;
                if self.landscape.p(&cand) >= threshold {
                    return true;
                }
            }
        }
        false
    }

    pub fn run(&mut self) -> Outcome {
        match self.stopping {
            Stopping::FixedDry(k) => {
                let mut dry = 0u32;
                while dry < k && !self.hit_cap {
                    if self.sweep() {
                        dry = 0;
                    } else {
                        dry += 1;
                    }
                }
            }
            Stopping::ConfirmedDry => {
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
            }
        }
        Outcome {
            final_len: self.current.len(),
            final_p: self.landscape.p(&self.current),
            eff_p: self.effective_p(),
            bug_retained: has_bug(&self.current),
            missed: self.oracle_missed(),
            execs: self.execs,
            accepts: self.accepts,
            gauntlet_rejects: self.gauntlet_rejects,
            rollbacks: self.rollbacks,
            hit_cap: self.hit_cap,
        }
    }
}

fn run_passes(sim: &mut Sim) {
    for k in [8usize, 4, 2, 1] {
        delete_chunks(sim, k);
    }
    zero_atoms(sim);
    minimize_atoms(sim);
}

fn delete_chunks(sim: &mut Sim, k: usize) {
    let mut i = 0;
    loop {
        if i + k > sim.current().len() {
            break;
        }
        let mut cand = sim.current().clone();
        cand.drain(i..i + k);
        if !sim.consider(cand) {
            i += 1;
        }
    }
}

fn zero_atoms(sim: &mut Sim) {
    let mut i = 0;
    while i < sim.current().len() {
        if sim.current()[i] != 0 {
            let mut cand = sim.current().clone();
            cand[i] = 0;
            sim.consider(cand);
        }
        i += 1;
    }
}

fn try_value(sim: &mut Sim, i: usize, val: u64) -> bool {
    let mut cand = sim.current().clone();
    cand[i] = val;
    sim.consider(cand)
}

fn minimize_atoms(sim: &mut Sim) {
    let mut i = 0;
    while i < sim.current().len() {
        let v = sim.current()[i];
        if v > 0 && !try_value(sim, i, 0) {
            let mut lo = 0u64;
            let mut hi = v;
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if try_value(sim, i, mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
        }
        i += 1;
    }
}
