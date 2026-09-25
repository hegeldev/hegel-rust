mod counter;
mod scenarios;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use hegel::{HealthCheck, Hegel, Phase, Settings, TestCase, Verbosity};

pub const SEED: u64 = 0x5EED_CAFE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Generate,
    Shrink,
}

pub struct Scenario {
    pub name: &'static str,
    pub kind: Kind,
    pub run: fn(&Config),
}

pub struct Config {
    pub test_cases: u64,
    pub executed: Cell<u64>,
}

impl Config {
    fn new(test_cases: u64) -> Self {
        Self {
            test_cases,
            executed: Cell::new(0),
        }
    }

    fn count<F: FnMut(TestCase)>(&self, mut body: F) -> impl FnMut(TestCase) {
        move |tc| {
            self.executed.set(self.executed.get() + 1);
            body(tc)
        }
    }
}

pub fn settings(cfg: &Config) -> Settings {
    Settings::new()
        .seed(Some(SEED))
        .database(None)
        .test_cases(cfg.test_cases)
        .verbosity(Verbosity::Quiet)
        .suppress_health_check([
            HealthCheck::FilterTooMuch,
            HealthCheck::TooSlow,
            HealthCheck::TestCasesTooLarge,
            HealthCheck::LargeInitialTestCase,
        ])
}

pub fn generate<F: FnMut(TestCase)>(cfg: &Config, body: F) {
    Hegel::new(cfg.count(body))
        .settings(settings(cfg).phases([Phase::Generate]))
        .run();
}

pub fn shrink<F: FnMut(TestCase)>(cfg: &Config, body: F) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(cfg.count(body))
            .settings(
                settings(cfg)
                    .phases([Phase::Generate, Phase::Shrink])
                    .print_blob(false),
            )
            .run();
    }));
    assert!(result.is_err(), "a shrink scenario must fail");
}

struct Measurement {
    name: String,
    kind: Kind,
    test_cases: u64,
    instructions: Option<u64>,
    wall: Vec<Duration>,
}

struct Options {
    filter: Vec<String>,
    walltime: bool,
    repeat: usize,
    test_cases: u64,
    json: Option<String>,
}

fn usage() -> ! {
    eprintln!(
        "usage: hegel-bench [--walltime] [--repeat K] [--test-cases N] [--json OUT] [FILTER...]\n       hegel-bench compare OLD.json NEW.json\n       hegel-bench list"
    );
    std::process::exit(2)
}

fn parse(args: &[String]) -> Options {
    let mut o = Options {
        filter: vec![],
        walltime: false,
        repeat: 5,
        test_cases: 1000,
        json: None,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--walltime" => o.walltime = true,
            "--repeat" => {
                i += 1;
                o.repeat = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "--test-cases" => {
                i += 1;
                o.test_cases = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "--json" => {
                i += 1;
                o.json = Some(args.get(i).cloned().unwrap_or_else(|| usage()));
            }
            s if s.starts_with("--") => usage(),
            s => o.filter.push(s.to_string()),
        }
        i += 1;
    }
    o
}

fn selected(o: &Options) -> Vec<&'static Scenario> {
    scenarios::all()
        .into_iter()
        .filter(|s| o.filter.is_empty() || o.filter.iter().any(|f| s.name.contains(f.as_str())))
        .collect()
}

fn measure(s: &Scenario, o: &Options) -> Measurement {
    let cfg = Config::new(o.test_cases);
    let mut m = Measurement {
        name: s.name.to_string(),
        kind: s.kind,
        test_cases: 0,
        instructions: None,
        wall: vec![],
    };
    if o.walltime {
        for _ in 0..o.repeat {
            cfg.executed.set(0);
            let t = Instant::now();
            (s.run)(&cfg);
            m.wall.push(t.elapsed());
        }
    } else {
        let mut c = counter::Instructions::new();
        c.start();
        (s.run)(&cfg);
        m.instructions = Some(c.stop());
    }
    m.test_cases = cfg.executed.get();
    m
}

fn per_case(m: &Measurement, v: f64) -> f64 {
    match m.kind {
        Kind::Generate => v / m.test_cases as f64,
        Kind::Shrink => v,
    }
}

fn unit(kind: Kind) -> &'static str {
    match kind {
        Kind::Generate => "/case",
        Kind::Shrink => "total",
    }
}

fn median(d: &mut [Duration]) -> Duration {
    d.sort();
    d[d.len() / 2]
}

fn write_json(path: &str, ms: &[Measurement]) {
    let mut out = String::from("{\n");
    for (i, m) in ms.iter().enumerate() {
        let value = match m.instructions {
            Some(n) => format!("\"instructions\": {n}"),
            None => {
                let mut w = m.wall.clone();
                format!(
                    "\"wall_median_ns\": {}, \"wall_min_ns\": {}",
                    median(&mut w).as_nanos(),
                    w.iter().min().unwrap().as_nanos()
                )
            }
        };
        out.push_str(&format!(
            "  \"{}\": {{\"kind\": \"{:?}\", \"test_cases\": {}, {}}}{}\n",
            m.name,
            m.kind,
            m.test_cases,
            value,
            if i + 1 < ms.len() { "," } else { "" }
        ));
    }
    out.push_str("}\n");
    fs::write(path, out).unwrap();
}

fn read_json(path: &str) -> BTreeMap<String, (String, u64, f64)> {
    let text = fs::read_to_string(path).unwrap();
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim().trim_end_matches(',');
        let Some((name, rest)) = line.split_once("\": {") else {
            continue;
        };
        let name = name.trim_start_matches('"').to_string();
        let field = |key: &str| -> Option<String> {
            let idx = rest.find(key)?;
            let after = &rest[idx + key.len()..];
            let after = after.trim_start_matches([':', ' ', '"']);
            let end = after
                .find([',', '}', '"'])
                .unwrap_or(after.len());
            Some(after[..end].to_string())
        };
        let kind = field("\"kind\"").unwrap();
        let test_cases: u64 = field("\"test_cases\"").unwrap().parse().unwrap();
        let value: f64 = field("\"instructions\"")
            .or_else(|| field("\"wall_median_ns\""))
            .unwrap()
            .parse()
            .unwrap();
        map.insert(name, (kind, test_cases, value));
    }
    map
}

fn compare(old: &str, new: &str) {
    let a = read_json(old);
    let b = read_json(new);
    println!(
        "{:<34} {:>16} {:>16} {:>9}",
        "scenario", "old", "new", "change"
    );
    for (name, (kind, tc, nv)) in &b {
        let Some((_, otc, ov)) = a.get(name) else {
            println!("{name:<34} {:>16} {nv:>16.0}", "-");
            continue;
        };
        let (ov, nv) = if kind == "Generate" {
            (ov / *otc as f64, nv / *tc as f64)
        } else {
            (*ov, *nv)
        };
        let pct = (nv - ov) / ov * 100.0;
        println!("{name:<34} {ov:>16.0} {nv:>16.0} {pct:>+8.2}%");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("compare") => {
            if args.len() != 3 {
                usage();
            }
            compare(&args[1], &args[2]);
            return;
        }
        Some("list") => {
            for s in scenarios::all() {
                println!("{:<34} {:?}", s.name, s.kind);
            }
            return;
        }
        _ => {}
    }
    let o = parse(&args);
    let mut results = vec![];
    let stdout = std::io::stdout();
    for s in selected(&o) {
        let m = measure(s, &o);
        let mut out = stdout.lock();
        match m.instructions {
            Some(n) => writeln!(
                out,
                "{:<34} {:>16} instructions {:<6} ({} total)",
                m.name,
                fmt_num(per_case(&m, n as f64)),
                unit(m.kind),
                fmt_num(n as f64)
            )
            .unwrap(),
            None => {
                let mut w = m.wall.clone();
                let med = median(&mut w);
                let min = *w.iter().min().unwrap();
                writeln!(
                    out,
                    "{:<34} {:>12.1} ns {:<6} median  {:>12.1} ns min  ({:?} median total)",
                    m.name,
                    per_case(&m, med.as_nanos() as f64),
                    unit(m.kind),
                    per_case(&m, min.as_nanos() as f64),
                    med
                )
                .unwrap()
            }
        }
        results.push(m);
    }
    if let Some(path) = &o.json {
        write_json(path, &results);
    }
}

fn fmt_num(v: f64) -> String {
    let s = format!("{:.0}", v);
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
