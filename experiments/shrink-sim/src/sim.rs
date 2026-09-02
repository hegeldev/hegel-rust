use std::collections::{HashMap, HashSet};

use crate::model::{has_bug, shortlex_less, Candidate, Landscape};
use crate::stats::{wilson_lcb, wilson_ucb, Evidence, Rng};

const Z: f64 = 1.96;
const GAUNTLET_CAP: u64 = 30;
const CONFIRM_RUNS: u64 = 20;
const VALIDATE_RUNS: u64 = 10;
const EXEC_CAP: u64 = 200_000;
const DRY_SWEEPS: u32 = 3;
const THRESHOLD_FLOOR: f64 = 0.05;

#[derive(Clone, Copy, Debug)]
pub enum Policy {
    Naive,
    PerCandidateN { n: u64 },
    FixedGauntlet { m: u64 },
    Ledger { gamma: f64, checkpoint: bool },
}

impl Policy {
    pub fn name(self) -> String {
        match self {
            Policy::Naive => "P0 naive".to_string(),
            Policy::PerCandidateN { n } => format!("P1 fail-within-{n}"),
            Policy::FixedGauntlet { m } => format!("P2 gauntlet-m{m}"),
            Policy::Ledger { gamma, checkpoint: false } => format!("P3 ledger g{gamma}"),
            Policy::Ledger { gamma, checkpoint: true } => format!("P4 ledger+ckpt g{gamma}"),
        }
    }
}

pub struct Outcome {
    pub final_len: usize,
    pub final_p: f64,
    pub bug_retained: bool,
    pub execs: u64,
    pub accepts: u64,
    pub gauntlet_rejects: u64,
    pub rollbacks: u64,
    pub hit_cap: bool,
}

pub struct Sim {
    landscape: Landscape,
    policy: Policy,
    rng: Rng,
    current: Candidate,
    ledger: HashMap<Candidate, Evidence>,
    poisoned: HashSet<Candidate>,
    accepted_since_snapshot: Vec<Candidate>,
    snapshot: Candidate,
    anchor: f64,
    execs: u64,
    accepts: u64,
    gauntlet_rejects: u64,
    rollbacks: u64,
    hit_cap: bool,
}

impl Sim {
    pub fn new(landscape: Landscape, policy: Policy, seed: u64, start: Candidate) -> Sim {
        let mut sim = Sim {
            landscape,
            policy,
            rng: Rng::new(seed),
            snapshot: start.clone(),
            current: start,
            ledger: HashMap::new(),
            poisoned: HashSet::new(),
            accepted_since_snapshot: Vec::new(),
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
        sim
    }

    pub fn current(&self) -> &Candidate {
        &self.current
    }

    fn execute(&mut self, c: &Candidate) -> bool {
        if self.execs >= EXEC_CAP {
            self.hit_cap = true;
            return false;
        }
        self.execs += 1;
        let fail = self.rng.f64() < self.landscape.p(c);
        let e = self.ledger.entry(c.clone()).or_default();
        e.runs += 1;
        if fail {
            e.fails += 1;
        }
        fail
    }

    fn evidence(&self, c: &Candidate) -> Evidence {
        self.ledger.get(c).copied().unwrap_or_default()
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
            Policy::Ledger { gamma, .. } => {
                if !self.execute(&cand) {
                    false
                } else {
                    let threshold = (gamma * self.anchor).max(THRESHOLD_FLOOR);
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
    }

    fn checkpoint(&mut self) {
        if !matches!(self.policy, Policy::Ledger { checkpoint: true, .. }) {
            return;
        }
        while self.evidence(&self.current).runs < VALIDATE_RUNS && !self.hit_cap {
            let c = self.current.clone();
            self.execute(&c);
        }
        let e = self.evidence(&self.current);
        let lcb = wilson_lcb(e.fails, e.runs, Z);
        if lcb < 0.5 * self.anchor {
            self.rollbacks += 1;
            for c in self.accepted_since_snapshot.drain(..) {
                self.poisoned.insert(c);
            }
            self.current = self.snapshot.clone();
        } else {
            self.snapshot = self.current.clone();
            self.accepted_since_snapshot.clear();
            if lcb > self.anchor {
                self.anchor = lcb;
            }
        }
    }

    pub fn run(&mut self) -> Outcome {
        let mut dry = 0u32;
        while dry < DRY_SWEEPS && !self.hit_cap {
            let before = self.accepts;
            run_passes(self);
            self.checkpoint();
            if self.accepts == before {
                dry += 1;
            } else {
                dry = 0;
            }
        }
        Outcome {
            final_len: self.current.len(),
            final_p: self.landscape.p(&self.current),
            bug_retained: has_bug(&self.current),
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
