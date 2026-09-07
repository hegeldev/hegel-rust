//! Experiment 014: multiplicity control for the ND statistics (decision 72).
//! Exact DP for the per-test rules plus a seeded Monte Carlo composition.
//! Spec and results: notes/experiments/014-fcr/notes.md

mod dp;
mod runsim;

use dp::{bar, evaluate, gauntlet, review_replays, seeded_anchor, ANCHOR_SEED_RUNS, GAUNTLET_CAP};

const FLUKES: [f64; 3] = [0.01, 0.02, 0.05];
const BUGS: [f64; 4] = [0.05, 0.1, 0.3, 0.9];

fn threshold_for(anchor: f64) -> f64 {
    let gamma = if anchor >= 0.8 { 1.0 } else { 0.8 };
    (gamma * anchor).max(0.05)
}

fn fresh() -> [(u64, u64, f64); 1] {
    [(0, 0, 1.0)]
}

fn recruited() -> [(u64, u64, f64); 1] {
    [(1, 1, 1.0)]
}

fn bar_section() {
    println!("## Discovery bar on plain counts, and the attempt composition\n");
    println!("| p | P(accept) | E[replays] |");
    println!("| --- | --- | --- |");
    for p in [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 0.9] {
        let r = evaluate(bar, p, &fresh(), 200);
        println!("| {p} | {:.4} | {:.1} |", r.p_accept, r.e_runs);
    }
    let alpha = evaluate(bar, 0.02, &fresh(), 200).p_accept;
    let power = evaluate(bar, 0.1, &fresh(), 200).p_accept;
    println!("\n| attempts F | P(false confirm, q=0.02) | P(confirm, p=0.1) |");
    println!("| --- | --- | --- |");
    for f in 1..=8i32 {
        println!(
            "| {f} | {:.4} | {:.3} |",
            1.0 - (1.0 - alpha).powi(f),
            1.0 - (1.0 - power).powi(f),
        );
    }
}

fn gauntlet_alpha(q: f64, threshold: f64, min_fails: u64, fast: bool) -> f64 {
    let rule = |n: u64, k: u64| gauntlet(n, k, threshold, min_fails, GAUNTLET_CAP);
    if fast {
        q * evaluate(rule, q, &recruited(), 200).p_accept
    } else {
        evaluate(rule, q, &fresh(), 200).p_accept
    }
}

fn gauntlet_section() {
    println!("\n## Gauntlet per-proposal false accept by failure minimum\n");
    println!("Unconditional per proposal: Fast mode needs the recruiting run to fail");
    println!("(and records it); Confirm mode drives every proposal to a bound verdict.\n");
    for &fast in &[true, false] {
        let mode = if fast { "Fast" } else { "Confirm" };
        println!("### {mode} sweep\n");
        println!("| anchor | threshold | q | m=4 | m=5 | m=6 | m=7 | m=8 |");
        println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
        for anchor in [0.05, 0.1, 0.3, 0.5, 0.9] {
            let t = threshold_for(anchor);
            for q in FLUKES {
                print!("| {anchor} | {t:.2} | {q} |");
                for m in 4..=8 {
                    print!(" {:.1e} |", gauntlet_alpha(q, t, m, fast));
                }
                println!();
            }
        }
    }
    println!("\n### Power and cost of the escalation (true candidate at rate p)\n");
    println!("Fast mode: accept-if-recruited is the loop's verdict; unconditional accept");
    println!("multiplies by the recruit failing, and replays count the recruit run.");
    println!("Thresholds: gamma x anchor at anchor = p (pessimistic; realized anchors");
    println!("seed lower), plus the realistic pairs — 0.053 from the bar's mean p = 0.1");
    println!("seed and 0.839 from a 20-run all-fail seed.\n");
    println!("| p | threshold | m | P(accept, recruited) | P(accept) | E[replays/proposal] |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for (p, t) in [
        (0.1, 0.053),
        (0.1, 0.08),
        (0.3, 0.24),
        (0.9, 0.839),
        (0.9, 0.9),
    ] {
        for m in 4..=7 {
            let rule = |n: u64, k: u64| gauntlet(n, k, t, m, GAUNTLET_CAP);
            let r = evaluate(rule, p, &recruited(), 200);
            println!(
                "| {p} | {t:.2} | {m} | {:.3} | {:.3} | {:.1} |",
                r.p_accept,
                p * r.p_accept,
                1.0 + p * (r.e_runs - 1.0),
            );
        }
    }
}

fn schedule_section() {
    println!("\n## Budget-based alpha spending per shrink\n");
    println!("Every proposal is charged its exact unconditional false-accept mass at the");
    println!("design fluke q0 = 0.02, the current threshold, and the sweep mode; when the");
    println!("remaining budget cannot afford the next proposal at the current failure");
    println!("minimum, the minimum escalates (to 8 at most). An unreachable threshold");
    println!("charges zero, so the high-anchor cost lottery (experiment 012) spends");
    println!("nothing. Proposals affordable per stage:\n");
    println!("| budget B | mode | threshold | at m=4 | m=5 | m=6 | m=7 | then m=8, per 10k |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for budget in [0.01f64, 0.02, 0.05] {
        for &fast in &[true, false] {
            let mode = if fast { "Fast" } else { "Confirm" };
            for t in [0.05, 0.08, 0.24] {
                let mut rem = budget;
                print!("| {budget} | {mode} | {t:.2} |");
                for m in 4..=7u64 {
                    let a = gauntlet_alpha(0.02, t, m, fast);
                    let afford = if a > 0.0 {
                        (rem / a).floor()
                    } else {
                        f64::INFINITY
                    };
                    // Stage width: spend half the remaining budget at each minimum,
                    // so every later stage keeps headroom.
                    let width = (afford / 2.0).floor();
                    rem -= width * a;
                    print!(" {width:.0} |");
                }
                let a8 = gauntlet_alpha(0.02, t, 8, fast);
                println!(" {:.5} |", 10_000.0 * a8);
            }
        }
    }
    println!("\nFlat m=4 exposure for contrast, 1-(1-alpha_4)^K at the floor:\n");
    println!("| mode | K=100 | K=1000 | K=10000 |");
    println!("| --- | --- | --- | --- |");
    for &fast in &[true, false] {
        let a = gauntlet_alpha(0.02, 0.05, 4, fast);
        println!(
            "| {} | {:.4} | {:.4} | {:.4} |",
            if fast { "Fast" } else { "Confirm" },
            1.0 - (1.0 - a).powi(100),
            1.0 - (1.0 - a).powi(1000),
            1.0 - (1.0 - a).powi(10000),
        );
    }
}

fn review_section() {
    println!("\n## Pooled-review confirmation: any-failure vs the bar\n");
    println!("OLD confirms an unconfirmed origin on any failure in the review; NEW hands");
    println!("the failing run to the standard evidence batch. Find = P(some review replay");
    println!("fails); confirm = P(origin confirms).\n");
    println!("| pool n | replays | rate x | find | OLD confirm | NEW confirm |");
    println!("| --- | --- | --- | --- | --- | --- |");
    let bar_at = |x: f64| evaluate(bar, x, &fresh(), 200).p_accept;
    for n in [1u64, 2, 5, 10] {
        let r = review_replays(n) as i32;
        for x in [0.02f64, 0.1, 0.3] {
            let find = 1.0 - (1.0 - x).powi(r);
            println!(
                "| {n} | {r} | {x} | {find:.3} | {find:.3} | {:.3} |",
                find * bar_at(x),
            );
        }
    }
}

fn anchor_section() {
    println!("\n## Seeded-anchor miscoverage P(LCB > p | accept), extension {ANCHOR_SEED_RUNS}\n");
    println!("| source | p | P(select) | miscoverage | mean anchor |");
    println!("| --- | --- | --- | --- | --- |");
    for p in BUGS {
        let r = evaluate(bar, p, &fresh(), 200);
        let (mis, mean) = seeded_anchor(&r.accept_states, p, ANCHOR_SEED_RUNS);
        println!(
            "| bar accept | {p} | {:.3} | {mis:.3} | {mean:.3} |",
            r.p_accept
        );
    }
    for p in BUGS {
        let t = threshold_for(p);
        let rule = |n: u64, k: u64| gauntlet(n, k, t, 4, GAUNTLET_CAP);
        let r = evaluate(rule, p, &recruited(), 200);
        let (mis, mean) = seeded_anchor(&r.accept_states, p, ANCHOR_SEED_RUNS);
        println!(
            "| gauntlet adopt | {p} | {:.3} | {mis:.3} | {mean:.3} |",
            p * r.p_accept
        );
    }
    for p in BUGS {
        // OLD review: stop at the first failure, seed from that evidence.
        let replays = review_replays(1);
        let mut states = Vec::new();
        for j in 0..replays {
            states.push((j + 1, 1, (1.0 - p).powi(j as i32) * p));
        }
        let select: f64 = states.iter().map(|&(_, _, m)| m).sum();
        let mut mis = 0.0;
        let mut mean = 0.0;
        for &(n, k, m) in &states {
            let lcb = dp::wilson(k as f64, n as f64, false);
            if lcb > p {
                mis += m;
            }
            mean += m * lcb;
        }
        println!(
            "| OLD review stop-at-fail | {p} | {select:.3} | {:.3} | {:.3} |",
            mis / select,
            mean / select,
        );
    }
}

fn main() {
    println!("# fcr-sim: multiplicity control operating characteristics\n");
    bar_section();
    gauntlet_section();
    schedule_section();
    review_section();
    anchor_section();
    runsim::run_section();
    runsim::mixed_section();
    let per_attempt_reject = 1.0 - evaluate(bar, 0.1, &fresh(), 200).p_accept;
    println!(
        "\nP(5 straight bar rejects at p = 0.1) = {:.4}",
        per_attempt_reject.powi(5)
    );
}
