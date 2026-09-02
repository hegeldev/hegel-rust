//! Experiment 002: cost of a fixate iteration with tree-serving off.
//! Spec and results: notes/experiments/002-cache-seam/notes.md

use hegel_c::__bench::fixate_cost_experiment;

fn main() {
    let reps = 10_000u32;
    println!("# fixate cost ({reps} replays of one recorded interesting case)\n");
    println!("| draws | serve | executions | total | ns/replay |");
    println!("| --- | --- | --- | --- | --- |");
    for draws in [10u32, 50, 200, 1000] {
        for serve in [true, false] {
            fixate_cost_experiment(serve, draws, 100);
            let report = fixate_cost_experiment(serve, draws, reps);
            println!(
                "| {} | {} | {} | {:.1?} | {:.0} |",
                draws,
                if serve { "tree" } else { "execute" },
                report.executions,
                report.total,
                report.total.as_nanos() as f64 / reps as f64,
            );
        }
    }
}
