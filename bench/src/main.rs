use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::exit;
use std::time::Instant;

use hegel::{HealthCheck, Hegel, Phase, Settings, TestCase, Verbosity};

mod workloads;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Generate,
    Shrink,
}

pub struct Workload {
    pub name: &'static str,
    pub kind: Kind,
    pub body: fn(&TestCase),
}

pub struct Options {
    pub test_cases: u64,
    pub seed: u64,
    pub trace: bool,
}

fn settings(kind: Kind, options: &Options) -> Settings {
    let phases = match kind {
        Kind::Generate => vec![Phase::Generate],
        Kind::Shrink => vec![Phase::Generate, Phase::Shrink],
    };
    Settings::new()
        .test_cases(options.test_cases)
        .seed(Some(options.seed))
        .database(None)
        .verbosity(if options.trace {
            Verbosity::Debug
        } else {
            Verbosity::Quiet
        })
        .print_blob(false)
        .phases(phases)
        .suppress_health_check([HealthCheck::TooSlow])
}

#[inline(never)]
pub fn measured(workload: &Workload, options: &Options) -> (u64, bool) {
    let cases = Cell::new(0u64);
    let body = workload.body;
    let run = Hegel::new(|tc: TestCase| {
        cases.set(cases.get() + 1);
        body(&tc);
    })
    .settings(settings(workload.kind, options));
    let failed = catch_unwind(AssertUnwindSafe(|| run.run())).is_err();
    (cases.get(), failed)
}

fn usage() -> ! {
    eprintln!(
        "usage: hegel-bench <workload> [--repeat N] [--test-cases N] [--seed N] [--trace]\n       hegel-bench --list\n\n--trace runs at debug verbosity, so two builds' outputs can be diffed."
    );
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    if args[0] == "--list" {
        for w in workloads::all() {
            let kind = match w.kind {
                Kind::Generate => "generate",
                Kind::Shrink => "shrink",
            };
            println!("{} {kind}", w.name);
        }
        return;
    }
    let mut repeat = 1u64;
    let mut options = Options {
        test_cases: 100,
        seed: 0,
        trace: false,
    };
    let mut i = 1;
    while i < args.len() {
        let value = || args.get(i + 1).and_then(|v| v.parse::<u64>().ok());
        match args[i].as_str() {
            "--trace" => {
                options.trace = true;
                i += 1;
                continue;
            }
            "--repeat" => repeat = value().unwrap_or_else(|| usage()),
            "--test-cases" => options.test_cases = value().unwrap_or_else(|| usage()),
            "--seed" => options.seed = value().unwrap_or_else(|| usage()),
            _ => usage(),
        }
        i += 2;
    }
    let Some(workload) = workloads::all().into_iter().find(|w| w.name == args[0]) else {
        eprintln!("unknown workload {:?}; --list shows them", args[0]);
        exit(2);
    };
    std::panic::set_hook(Box::new(|_| {}));
    for n in 0..repeat {
        let start = Instant::now();
        let (cases, failed) = measured(&workload, &options);
        let elapsed = start.elapsed();
        let expected_failure = workload.kind == Kind::Shrink;
        if failed != expected_failure {
            eprintln!(
                "workload {} {} but was expected {}",
                workload.name,
                if failed { "failed" } else { "passed" },
                if expected_failure {
                    "to fail"
                } else {
                    "to pass"
                }
            );
            exit(1);
        }
        println!(
            "hegel-bench workload={} repeat={n} cases={cases} elapsed_us={}",
            workload.name,
            elapsed.as_micros()
        );
    }
}
