mod landscape;
mod newp;
mod oldp;
mod stats;

use landscape::{Landscape, ALL};
use std::fmt::Write as _;

const BASE_SEED: u64 = 0x5EED2026;
const TRIALS: u64 = 200;

fn dp_table(out: &mut String) {
    let qs = [0.45, 0.50, 0.55, 0.60, 0.75, 0.90];
    writeln!(
        out,
        "Adoption gate (exact binomial, no simulation): minimum beats m_k with Wilson"
    )
    .unwrap();
    writeln!(out, "LCB(m_k/k, z=1.96) > 0.5, and P(Bin(k, q) >= m_k):").unwrap();
    let mut header = format!("{:>4} {:>5}", "k", "m_k");
    for q in qs {
        write!(header, " {:>8}", format!("q={q:.2}")).unwrap();
    }
    writeln!(out, "{header}").unwrap();
    for k in [10u64, 20, 30] {
        let m = stats::min_beats(k, 1.96);
        let mut row = format!("{k:>4} {m:>5}");
        for q in qs {
            write!(row, " {:>8.4}", stats::binom_tail_ge(k, q, m)).unwrap();
        }
        writeln!(out, "{row}").unwrap();
    }
    writeln!(out).unwrap();
}

fn old_section(out: &mut String) {
    writeln!(
        out,
        "OLD policy: single-run hill climb on max-ever score, {} runs/trial",
        oldp::BUDGET
    )
    .unwrap();
    writeln!(
        out,
        "curse_bias = s_best - true_mean(x_best); frozen = zero accepted steps in the"
    )
    .unwrap();
    writeln!(
        out,
        "final 50% of the budget while gradient remained above x_best (rate over trials);"
    )
    .unwrap();
    writeln!(
        out,
        "the climb ends when neither direction's first probe succeeds twice in a row"
    )
    .unwrap();
    writeln!(
        out,
        "{:<8} {:>5} {:>8} {:>10} {:>11} {:>8} {:>7} {:>7}",
        "land", "miss", "x_fin", "truemean", "curse_bias", "frozen", "steps", "runs"
    )
    .unwrap();
    let mut cells: Vec<(Landscape, f64)> = ALL.iter().map(|&l| (l, 0.0)).collect();
    cells.push((Landscape::Lin, 0.3));
    cells.push((Landscape::Flat, 0.3));
    for (land, miss) in cells {
        let os: Vec<_> = (0..TRIALS)
            .map(|i| oldp::run_trial(land, miss, BASE_SEED + i))
            .collect();
        let n = TRIALS as f64;
        let x = os.iter().map(|o| o.final_x as f64).sum::<f64>() / n;
        let tm = os.iter().map(|o| o.true_mean).sum::<f64>() / n;
        let cb = os.iter().map(|o| o.curse_bias).sum::<f64>() / n;
        let fr = os.iter().filter(|o| o.frozen).count() as f64 / n;
        let st = os.iter().map(|o| o.steps as f64).sum::<f64>() / n;
        let ru = os.iter().map(|o| o.runs as f64).sum::<f64>() / n;
        writeln!(
            out,
            "{:<8} {:>5.1} {:>8.1} {:>10.1} {:>11.1} {:>8.3} {:>7.1} {:>7.1}",
            land.name(),
            miss,
            x,
            tm,
            cb,
            fr,
            st,
            ru
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn new_header(out: &mut String) {
    writeln!(
        out,
        "{:<8} {:>3} {:>3} {:>5} {:>8} {:>9} {:>9} {:>7} {:>9} {:>8} {:>8} {:>5}",
        "land",
        "H",
        "R",
        "miss",
        "x_fin",
        "prog_mu",
        "prog_med",
        "adopts",
        "refdrift",
        "rep/tr",
        "rep/ad",
        "dead"
    )
    .unwrap();
}

fn new_row(out: &mut String, land: Landscape, holdout: u64, races: u64, miss: f64) {
    let os: Vec<_> = (0..TRIALS)
        .map(|i| newp::run_trial(land, miss, holdout, races, BASE_SEED + i))
        .collect();
    let n = TRIALS as f64;
    let progs: Vec<f64> = os.iter().map(|o| o.progress).collect();
    let prog_mu = progs.iter().sum::<f64>() / n;
    let prog_med = stats::median_upper(progs);
    let adopts: u64 = os.iter().map(|o| o.adopts).sum();
    let adopts_mu = adopts as f64 / n;
    let x_fin = os.iter().map(|o| o.final_x as f64).sum::<f64>() / n;
    let live: Vec<f64> = os
        .iter()
        .filter(|o| !o.dead)
        .map(|o| o.ref_drift)
        .collect();
    let drift = if live.is_empty() {
        f64::NAN
    } else {
        live.iter().sum::<f64>() / live.len() as f64
    };
    let reps: u64 = os.iter().map(|o| o.replays).sum();
    let rep_tr = reps as f64 / n;
    let rep_ad = if adopts == 0 {
        "-".to_string()
    } else {
        format!("{:.0}", reps as f64 / adopts as f64)
    };
    let dead = os.iter().filter(|o| o.dead).count();
    writeln!(
        out,
        "{:<8} {:>3} {:>3} {:>5.1} {:>8.1} {:>9.1} {:>9.1} {:>7.2} {:>9.2} {:>8.0} {:>8} {:>5}",
        land.name(),
        holdout,
        races,
        miss,
        x_fin,
        prog_mu,
        prog_med,
        adopts_mu,
        drift,
        rep_tr,
        rep_ad,
        dead
    )
    .unwrap();
}

fn main() {
    let mut out = String::new();
    writeln!(
        out,
        "target-sim: score targeting policies under nondeterministic scores"
    )
    .unwrap();
    writeln!(
        out,
        "base seed: {BASE_SEED:#x} ({BASE_SEED}); trial i uses seed base+i; {TRIALS} trials/cell"
    )
    .unwrap();
    writeln!(
        out,
        "L-heavy: score = x + 5*T, T ~ Student-t(df=2) via exact inverse CDF T = (2u-1)/sqrt(2u(1-u))"
    )
    .unwrap();
    writeln!(
        out,
        "median = sorted[k/2] (upper middle); refdrift = r_final - true_median(x0), the"
    )
    .unwrap();
    writeln!(
        out,
        "reference-honesty gauge on L-flat (tracks the moved position elsewhere)"
    )
    .unwrap();
    writeln!(out).unwrap();

    dp_table(&mut out);
    old_section(&mut out);

    writeln!(out, "NEW policy: HOLDOUT sweep (RACES = 4, miss = 0.0)").unwrap();
    new_header(&mut out);
    for land in ALL {
        for h in [10, 20, 30] {
            new_row(&mut out, land, h, 4, 0.0);
        }
    }
    writeln!(out).unwrap();

    writeln!(out, "NEW policy: RACES sweep (HOLDOUT = 20, miss = 0.0)").unwrap();
    new_header(&mut out);
    for land in ALL {
        for r in [2, 4, 8] {
            new_row(&mut out, land, 20, r, 0.0);
        }
    }
    writeln!(out).unwrap();

    writeln!(out, "NEW policy: miss_rate sweep (HOLDOUT = 20, RACES = 4)").unwrap();
    new_header(&mut out);
    for land in [Landscape::Lin, Landscape::Flat] {
        for miss in [0.0, 0.3] {
            new_row(&mut out, land, 20, 4, miss);
        }
    }

    print!("{out}");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/results.txt");
    std::fs::write(path, &out).unwrap();
    eprintln!("results written to {path}");
}
