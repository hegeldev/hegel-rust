//! Experiment 005A: confirmation-bar operating characteristics by exact DP.
//! Spec and results: notes/experiments/005-lifecycle/notes.md

use std::collections::HashMap;

const PS: [f64; 7] = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 0.9];
const Z: f64 = 1.96;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Decision {
    Accept,
    Reject,
    Continue,
}

enum Rule {
    Flat { k: u32, b: u32 },
    TwoStage { gate_n: u32, gate_k: u32, k: u32, b: u32 },
    Sprt { alpha: f64, beta: f64, cap: u32 },
    Wilson { lo: f64, hi: f64, cap: u32 },
}

const P0: f64 = 0.02;
const P1: f64 = 0.1;

impl Rule {
    fn name(&self) -> String {
        match self {
            Rule::Flat { k, b } => format!("flat {k}/{b}"),
            Rule::TwoStage { gate_n, gate_k, k, b } => {
                format!("gate {gate_k}/{gate_n} then {k}/{b}")
            }
            Rule::Sprt { alpha, beta, cap } => format!("sprt a={alpha} b={beta} cap={cap}"),
            Rule::Wilson { lo, hi, cap } => format!("wilson {lo}/{hi} cap={cap}"),
        }
    }

    fn decide(&self, n: u32, k: u32) -> Decision {
        match *self {
            Rule::Flat { k: kmin, b } => {
                if k >= kmin {
                    Decision::Accept
                } else if k + (b - n) < kmin {
                    Decision::Reject
                } else {
                    Decision::Continue
                }
            }
            Rule::TwoStage { gate_n, gate_k, k: kmin, b } => {
                if k >= kmin {
                    Decision::Accept
                } else if n >= gate_n && k < gate_k {
                    Decision::Reject
                } else if k + (b - n) < kmin {
                    Decision::Reject
                } else {
                    Decision::Continue
                }
            }
            Rule::Sprt { alpha, beta, cap } => {
                let llr = k as f64 * (P1 / P0).ln()
                    + (n - k) as f64 * ((1.0 - P1) / (1.0 - P0)).ln();
                let upper = ((1.0 - beta) / alpha).ln();
                let lower = (beta / (1.0 - alpha)).ln();
                if llr >= upper {
                    Decision::Accept
                } else if llr <= lower {
                    Decision::Reject
                } else if n >= cap {
                    if llr > 0.0 {
                        Decision::Accept
                    } else {
                        Decision::Reject
                    }
                } else {
                    Decision::Continue
                }
            }
            Rule::Wilson { lo, hi, cap } => {
                if n == 0 {
                    return Decision::Continue;
                }
                let nf = n as f64;
                let kf = k as f64;
                let denom = nf + Z * Z;
                let center = (kf + Z * Z / 2.0) / denom;
                let half = Z / denom * (kf * (nf - kf) / nf + Z * Z / 4.0).sqrt();
                if center - half >= lo {
                    Decision::Accept
                } else if center + half < hi {
                    Decision::Reject
                } else if n >= cap {
                    if kf / nf >= 0.05 {
                        Decision::Accept
                    } else {
                        Decision::Reject
                    }
                } else {
                    Decision::Continue
                }
            }
        }
    }
}

fn evaluate(rule: &Rule, p: f64) -> (f64, f64) {
    let mut mass: HashMap<u32, f64> = HashMap::from([(0, 1.0)]);
    let mut p_accept = 0.0;
    let mut e_runs = 0.0;
    for n in 0..=200u32 {
        let mut next: HashMap<u32, f64> = HashMap::new();
        for (&k, &m) in &mass {
            match rule.decide(n, k) {
                Decision::Accept => {
                    p_accept += m;
                    e_runs += m * n as f64;
                }
                Decision::Reject => {
                    e_runs += m * n as f64;
                }
                Decision::Continue => {
                    *next.entry(k + 1).or_insert(0.0) += m * p;
                    *next.entry(k).or_insert(0.0) += m * (1.0 - p);
                }
            }
        }
        mass = next;
        if mass.is_empty() {
            break;
        }
    }
    assert!(mass.is_empty(), "rule {} did not terminate", rule.name());
    (p_accept, e_runs)
}

fn main() {
    let rules = [
        Rule::Flat { k: 2, b: 20 },
        Rule::Flat { k: 3, b: 20 },
        Rule::Flat { k: 3, b: 30 },
        Rule::Flat { k: 4, b: 30 },
        Rule::Flat { k: 3, b: 50 },
        Rule::Flat { k: 4, b: 50 },
        Rule::TwoStage { gate_n: 10, gate_k: 1, k: 3, b: 30 },
        Rule::TwoStage { gate_n: 10, gate_k: 1, k: 4, b: 40 },
        Rule::TwoStage { gate_n: 15, gate_k: 1, k: 3, b: 40 },
        Rule::Sprt { alpha: 0.05, beta: 0.05, cap: 50 },
        Rule::Sprt { alpha: 0.01, beta: 0.05, cap: 80 },
        Rule::Sprt { alpha: 0.01, beta: 0.3, cap: 50 },
        Rule::Wilson { lo: 0.02, hi: 0.1, cap: 40 },
    ];

    println!("# confirm-bar operating characteristics (exact DP)\n");
    println!("## P(accept) by true p\n");
    print!("| rule |");
    for p in PS {
        print!(" p={p} |");
    }
    println!();
    print!("| --- |");
    for _ in PS {
        print!(" --- |");
    }
    println!();
    for rule in &rules {
        print!("| {} |", rule.name());
        for p in PS {
            let (acc, _) = evaluate(rule, p);
            print!(" {:.3} |", acc);
        }
        println!();
    }

    println!("\n## E[replays] by true p\n");
    print!("| rule |");
    for p in PS {
        print!(" p={p} |");
    }
    println!();
    print!("| --- |");
    for _ in PS {
        print!(" --- |");
    }
    println!();
    for rule in &rules {
        print!("| {} |", rule.name());
        for p in PS {
            let (_, runs) = evaluate(rule, p);
            print!(" {:.1} |", runs);
        }
        println!();
    }

    println!("\n## Run-level arithmetic (F fluke discoveries per run at p=0.02)\n");
    println!("| rule | alpha=P(acc|0.02) | P(false acc) F=2 | F=5 | wasted replays F=5 | P(run confirms p=0.1 bug within 3 discoveries) |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for rule in &rules {
        let (alpha, cost_noise) = evaluate(rule, 0.02);
        let (power, _) = evaluate(rule, 0.1);
        println!(
            "| {} | {:.4} | {:.3} | {:.3} | {:.0} | {:.3} |",
            rule.name(),
            alpha,
            1.0 - (1.0 - alpha).powi(2),
            1.0 - (1.0 - alpha).powi(5),
            5.0 * cost_noise,
            1.0 - (1.0 - power).powi(3),
        );
    }
}
