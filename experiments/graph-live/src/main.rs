//! Graph-era live experiment harness (experiment 020): 016's `live-set`
//! harness carried to the counterexample graph, one process per run, driven
//! by `drive.py`.
//!
//! Subcommands:
//! - `discover-<body> <dbdir> <seed>`: Generate+Shrink into a fresh database.
//! - `reuse-<body> <dbdir> <seed>`: Reuse+Shrink from an existing database.
//! - `replay-<body> <blob>`: replay a reproduce blob with the database disabled.
//! - `blobinfo-<body> <blob>`: decode a blob's graph, enumerate its paths and
//!   judge each by the body's own predicate.
//!
//! Every run prints `EXECUTIONS: <n>` (body invocations) and
//! `RESULT: FAILED|PASSED`.

use hegel::generators as gs;
use hegel::stateful::machine;
use hegel::{Hegel, NondeterminismStrictness, Phase, Settings, TestCase, Verbosity};
use hegel_c::__bench::{blob_graph, BlobGraph, ChoiceValue, ToPrimitive};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const LABEL_INT: u64 = gs::label_from_name("hegel.integer");
const LABEL_BOOL: u64 = gs::label_from_name("hegel.boolean");
const LABEL_PIECE: u64 = 1001;
const LABEL_ARM: u64 = 1002;
const LOOP_CONTINUE: f64 = 0.75;
const MAX_PATHS: usize = 100_000;

static EXECUTIONS: AtomicUsize = AtomicUsize::new(0);
static FIRST_FAILURE_AT: AtomicUsize = AtomicUsize::new(0);
static K: AtomicUsize = AtomicUsize::new(2);
static HIDDEN: Mutex<u64> = Mutex::new(0x9E3779B97F4A7C15);

fn seed_hidden(seed: u64) {
    let mut s = HIDDEN.lock().unwrap();
    *s = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
}

fn hidden_next() -> u64 {
    let mut s = HIDDEN.lock().unwrap();
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x
}

/// A coin flip the engine never sees: process-global xorshift64, P(true) = 0.5.
fn hidden_coin() -> bool {
    (hidden_next() >> 63) == 1
}

fn hidden_f64() -> f64 {
    (hidden_next() >> 11) as f64 / (1u64 << 53) as f64
}

fn k() -> usize {
    K.load(Ordering::SeqCst)
}

fn count_execution() {
    EXECUTIONS.fetch_add(1, Ordering::SeqCst);
}

fn fail_if(fail: bool) {
    if fail {
        FIRST_FAILURE_AT
            .compare_exchange(0, EXECUTIONS.load(Ordering::SeqCst), Ordering::SeqCst, Ordering::SeqCst)
            .ok();
    }
    assert!(!fail, "branch bug");
}

fn small_int() -> impl gs::PrintableGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(100)
}

struct RacyCounter {
    value: AtomicI64,
    increments: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl RacyCounter {
    #[rule]
    fn racy_increment(&self, _: TestCase) {
        let value = self.value.load(Ordering::SeqCst);
        std::thread::yield_now();
        self.value.store(value + 1, Ordering::SeqCst);
        self.increments.fetch_add(1, Ordering::SeqCst);
    }

    #[invariant]
    fn no_lost_updates(&self, _: TestCase) {
        assert_eq!(
            self.value.load(Ordering::SeqCst),
            self.increments.load(Ordering::SeqCst)
        );
    }
}

fn racy_body(tc: TestCase) {
    count_execution();
    let m = RacyCounter {
        value: AtomicI64::new(0),
        increments: AtomicI64::new(0),
    };
    machine(m)
        .min_concurrency(2)
        .max_concurrency(4)
        .run_concurrent(tc);
}

static CLONE_CALLS: AtomicI64 = AtomicI64::new(0);

fn clone_flaky_body(tc: TestCase) {
    count_execution();
    let child = tc.clone();
    let x: i64 = child.draw(gs::integers::<i64>().min_value(0).max_value(1000));
    let call = CLONE_CALLS.fetch_add(1, Ordering::SeqCst);
    if call % 3 == 0 {
        assert!(x < 500, "clone-flaky: x = {x}");
    }
}

fn branch_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let fail = if hidden_coin() {
        let b = tc.draw(gs::booleans());
        let x = tc.draw(small_int());
        a && b && x >= 60
    } else {
        let y = tc.draw(small_int());
        let z = tc.draw(small_int());
        y >= 60 && z >= 60
    };
    fail_if(fail);
}

fn hot_piece(tc: &TestCase) -> bool {
    if hidden_coin() {
        tc.draw(gs::booleans())
    } else {
        tc.draw(small_int()) >= 60
    }
}

fn twobranch_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let first = hot_piece(&tc);
    let second = hot_piece(&tc);
    fail_if(a && first && second);
}

fn shift_piece(tc: &TestCase) -> bool {
    if hidden_coin() {
        tc.draw(gs::booleans())
    } else {
        let x = tc.draw(small_int());
        tc.draw(gs::booleans());
        x >= 60
    }
}

fn kblock_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let mut hot = true;
    for _ in 0..k() {
        hot &= hot_piece(&tc);
    }
    fail_if(a && hot);
}

fn kshift_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let mut hot = true;
    for _ in 0..k() {
        hot &= shift_piece(&tc);
    }
    fail_if(a && hot);
}

/// 019's piece: a PIECE span around a hidden coin choosing a bool piece (hot
/// iff true) or an int piece (hot iff `>= 60`); under `shift` the int arm is
/// an ARM span drawing the int and an ignored bool.
fn spanned_piece(tc: &TestCase, shift: bool) -> bool {
    tc.start_span(LABEL_PIECE);
    let hot = if hidden_coin() {
        tc.draw(gs::booleans())
    } else if shift {
        tc.start_span(LABEL_ARM);
        let x = tc.draw(small_int());
        tc.draw(gs::booleans());
        tc.stop_span(false);
        x >= 60
    } else {
        tc.draw(small_int()) >= 60
    };
    tc.stop_span(false);
    hot
}

fn block_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let mut hot = true;
    for _ in 0..k() {
        hot &= spanned_piece(&tc, false);
    }
    fail_if(a && hot);
}

fn shift_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let mut hot = true;
    for _ in 0..k() {
        hot &= spanned_piece(&tc, true);
    }
    fail_if(a && hot);
}

fn list_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let n = tc.draw(gs::integers::<i64>().min_value(0).max_value(k() as i64));
    let mut hot = true;
    for _ in 0..n {
        hot &= spanned_piece(&tc, false);
    }
    fail_if(a && n >= 1 && hot);
}

fn loop_body(tc: TestCase) {
    count_execution();
    let a = tc.draw(gs::booleans());
    let mut hot = true;
    let mut m = 0;
    while m < k() && hidden_f64() < LOOP_CONTINUE {
        hot &= spanned_piece(&tc, false);
        m += 1;
    }
    let z = tc.draw(gs::booleans());
    fail_if(a && hot && z);
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Racy,
    Clone,
    Branch,
    KBlock,
    KShift,
    Block,
    Shift,
    List,
    Loop,
}

#[derive(Clone, Copy, Debug)]
struct Body {
    kind: Kind,
    k: usize,
}

impl Body {
    fn parse(name: &str) -> Body {
        let split = name.find(|c: char| c.is_ascii_digit()).unwrap_or(name.len());
        let k: usize = name[split..].parse().unwrap_or(0);
        let kind = match &name[..split] {
            "racy" => Kind::Racy,
            "clone" => Kind::Clone,
            "branch" => Kind::Branch,
            "twobranch" => return Body { kind: Kind::KBlock, k: 2 },
            "kblock" => Kind::KBlock,
            "kshift" => Kind::KShift,
            "block" => Kind::Block,
            "shift" => Kind::Shift,
            "list" => Kind::List,
            "loop" => Kind::Loop,
            other => panic!("unknown body {other}"),
        };
        Body { kind, k }
    }

    fn function(&self, name: &str) -> fn(TestCase) {
        K.store(self.k, Ordering::SeqCst);
        match self.kind {
            Kind::Racy => racy_body,
            Kind::Clone => clone_flaky_body,
            Kind::Branch => branch_body,
            Kind::KBlock if name == "twobranch" => twobranch_body,
            Kind::KBlock => kblock_body,
            Kind::KShift => kshift_body,
            Kind::Block => block_body,
            Kind::Shift => shift_body,
            Kind::List => list_body,
            Kind::Loop => loop_body,
        }
    }

    fn test_cases(&self, name: &str) -> u64 {
        match self.kind {
            Kind::Racy | Kind::Clone | Kind::Branch => 200,
            Kind::KBlock if name == "twobranch" => 200,
            _ if self.k >= 8 => 5000,
            _ => 2000,
        }
    }
}

type Frame = (u64, usize);

struct Step {
    addr: Vec<Frame>,
    value: ChoiceValue,
}

struct Cursor<'a> {
    steps: &'a [Step],
    i: usize,
    top_bools: usize,
    top_ints: usize,
}

impl<'a> Cursor<'a> {
    fn new(steps: &'a [Step]) -> Cursor<'a> {
        Cursor {
            steps,
            i: 0,
            top_bools: 0,
            top_ints: 0,
        }
    }

    fn peek(&self) -> Option<&'a Step> {
        self.steps.get(self.i)
    }

    fn take(&mut self, addr: &[Frame]) -> Option<&'a ChoiceValue> {
        let s = self.steps.get(self.i)?;
        if s.addr != addr {
            return None;
        }
        self.i += 1;
        Some(&s.value)
    }

    fn bool_at(&mut self, addr: &[Frame]) -> Option<bool> {
        match self.take(addr)? {
            ChoiceValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    fn int_at(&mut self, addr: &[Frame]) -> Option<i64> {
        match self.take(addr)? {
            ChoiceValue::Integer(x) => x.to_i64(),
            _ => None,
        }
    }

    fn next_top_label(&self) -> Option<u64> {
        let s = self.peek()?;
        (s.addr.len() == 1).then(|| s.addr[0].0)
    }

    fn top_bool(&mut self) -> Option<bool> {
        let b = self.bool_at(&[(LABEL_BOOL, self.top_bools)])?;
        self.top_bools += 1;
        Some(b)
    }

    fn top_int(&mut self) -> Option<i64> {
        let x = self.int_at(&[(LABEL_INT, self.top_ints)])?;
        self.top_ints += 1;
        Some(x)
    }

    fn done(&self) -> Option<()> {
        (self.i == self.steps.len()).then_some(())
    }
}

fn bare_piece(c: &mut Cursor, shift: bool) -> Option<bool> {
    match c.next_top_label()? {
        LABEL_BOOL => c.top_bool(),
        LABEL_INT => {
            let x = c.top_int()?;
            if shift {
                c.top_bool()?;
            }
            Some(x >= 60)
        }
        _ => None,
    }
}

fn spanned_piece_at(c: &mut Cursor, j: usize, shift: bool) -> Option<bool> {
    let p = (LABEL_PIECE, j);
    let next = c.peek()?;
    if next.addr.first() != Some(&p) {
        return None;
    }
    match next.addr.get(1)?.0 {
        LABEL_BOOL => c.bool_at(&[p, (LABEL_BOOL, 0)]),
        LABEL_INT if !shift => c.int_at(&[p, (LABEL_INT, 0)]).map(|x| x >= 60),
        LABEL_ARM if shift => {
            let x = c.int_at(&[p, (LABEL_ARM, 0), (LABEL_INT, 0)])?;
            c.bool_at(&[p, (LABEL_ARM, 0), (LABEL_BOOL, 0)])?;
            Some(x >= 60)
        }
        _ => None,
    }
}

/// The body's verdict on a whole path, read off its structure and values:
/// `Some(fail)` for a path some run of the body produces, `None` otherwise.
fn predicate(body: Body, steps: &[Step]) -> Option<bool> {
    let mut c = Cursor::new(steps);
    let a = c.top_bool()?;
    let mut hot = true;
    match body.kind {
        Kind::Racy | Kind::Clone => return None,
        Kind::Branch => {
            let fail = match c.next_top_label()? {
                LABEL_BOOL => {
                    let b = c.top_bool()?;
                    let x = c.top_int()?;
                    a && b && x >= 60
                }
                LABEL_INT => {
                    let y = c.top_int()?;
                    let z = c.top_int()?;
                    y >= 60 && z >= 60
                }
                _ => return None,
            };
            c.done()?;
            return Some(fail);
        }
        Kind::KBlock | Kind::KShift => {
            for _ in 0..body.k {
                hot &= bare_piece(&mut c, body.kind == Kind::KShift)?;
            }
        }
        Kind::Block | Kind::Shift => {
            for j in 0..body.k {
                hot &= spanned_piece_at(&mut c, j, body.kind == Kind::Shift)?;
            }
        }
        Kind::List => {
            let n = c.top_int()?;
            if n < 0 || n > body.k as i64 {
                return None;
            }
            for j in 0..n as usize {
                hot &= spanned_piece_at(&mut c, j, false)?;
            }
            c.done()?;
            return Some(a && n >= 1 && hot);
        }
        Kind::Loop => {
            let mut j = 0;
            while c
                .peek()
                .is_some_and(|s| s.addr.first() == Some(&(LABEL_PIECE, j)))
            {
                if j >= body.k {
                    return None;
                }
                hot &= spanned_piece_at(&mut c, j, false)?;
                j += 1;
            }
            let z = c.top_bool()?;
            c.done()?;
            return Some(a && hot && z);
        }
    }
    c.done()?;
    Some(a && hot)
}

fn shape(steps: &[Step]) -> String {
    steps
        .iter()
        .map(|s| match s.value {
            ChoiceValue::Integer(_) => 'i',
            ChoiceValue::Boolean(_) => 'b',
            ChoiceValue::Float(_) => 'f',
            ChoiceValue::Bytes(_) => 'y',
            ChoiceValue::String(_) => 's',
            ChoiceValue::Clone(_) => 'c',
        })
        .collect()
}

fn flat_len(steps: &[Step]) -> usize {
    steps
        .iter()
        .map(|s| match &s.value {
            ChoiceValue::Clone(r) => 1 + r.flat_len(),
            _ => 1,
        })
        .sum()
}

struct Paths {
    paths: Vec<Vec<Step>>,
    cyclic: usize,
    capped: bool,
}

/// Every Start→End path of the graph in edge order, an edge back onto the
/// current path counted as cyclic and not followed.
fn enumerate_paths(g: &BlobGraph) -> Paths {
    let mut out = Paths {
        paths: Vec::new(),
        cyclic: 0,
        capped: false,
    };
    let mut path: Vec<Step> = Vec::new();
    let mut on_path = vec![false; g.nodes.len()];
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    on_path[0] = true;
    while let Some(&mut (n, ref mut i)) = stack.last_mut() {
        if n == 1 {
            out.paths.push(path.iter().map(|s| Step { addr: s.addr.clone(), value: s.value.clone() }).collect());
            if out.paths.len() >= MAX_PATHS {
                out.capped = true;
                break;
            }
            on_path[n] = false;
            stack.pop();
            path.pop();
            continue;
        }
        if *i >= g.nodes[n].edges.len() {
            on_path[n] = false;
            stack.pop();
            path.pop();
            continue;
        }
        let e = &g.nodes[n].edges[*i];
        *i += 1;
        if on_path[e.target] {
            out.cyclic += 1;
            continue;
        }
        path.push(Step { addr: e.addr.clone(), value: e.value.clone() });
        on_path[e.target] = true;
        stack.push((e.target, 0));
    }
    out
}

fn reachable(g: &BlobGraph) -> (usize, usize) {
    let mut seen = vec![false; g.nodes.len()];
    let mut order = vec![0usize];
    seen[0] = true;
    let mut i = 0;
    while i < order.len() {
        let n = order[i];
        i += 1;
        for e in &g.nodes[n].edges {
            if !seen[e.target] {
                seen[e.target] = true;
                order.push(e.target);
            }
        }
    }
    (order.len(), order.iter().map(|&n| g.nodes[n].edges.len()).sum())
}

fn blobinfo(body: Body, blob: &str) {
    let Some(g) = blob_graph(blob) else {
        println!("ND: false");
        return;
    };
    println!("ND: true");
    println!("LONGEST: {}", g.longest);
    let (nodes, edges) = reachable(&g);
    println!("NODES: {nodes}");
    println!("EDGES: {edges}");
    let paths = enumerate_paths(&g);
    println!("PATHS: {}", paths.paths.len());
    println!("PATHS_CAPPED: {}", paths.capped);
    println!("CYCLIC: {}", paths.cyclic);
    let mut right = 0;
    let mut passing = 0;
    let mut malformed = 0;
    let mut shapes_all = BTreeSet::new();
    let mut shapes_right = BTreeSet::new();
    let mut right_lens = Vec::new();
    for p in &paths.paths {
        shapes_all.insert(shape(p));
        match predicate(body, p) {
            Some(true) => {
                right += 1;
                shapes_right.insert(shape(p));
                right_lens.push(flat_len(p));
            }
            Some(false) => passing += 1,
            None => malformed += 1,
        }
    }
    let lens: Vec<usize> = paths.paths.iter().map(|p| flat_len(p)).collect();
    println!("SHAPES_ALL: {}", shapes_all.len());
    if matches!(body.kind, Kind::Racy | Kind::Clone) {
        println!("VERDICT: n/a");
    } else {
        println!("VERDICT: judged");
        println!("RIGHT: {right}");
        println!("PASSING: {passing}");
        println!("MALFORMED: {malformed}");
        println!("SHAPES_RIGHT: {}", shapes_right.len());
    }
    println!(
        "MIN_LEN: {}",
        lens.iter().min().map(|x| x.to_string()).unwrap_or_default()
    );
    println!(
        "MAX_LEN: {}",
        lens.iter().max().map(|x| x.to_string()).unwrap_or_default()
    );
    println!(
        "MIN_RIGHT_LEN: {}",
        right_lens.iter().min().map(|x| x.to_string()).unwrap_or_default()
    );
    let mut listed: Vec<String> = paths
        .paths
        .iter()
        .take(64)
        .map(|p| {
            let verdict = match predicate(body, p) {
                Some(true) => "F",
                Some(false) => "P",
                None => "?",
            };
            let values: Vec<String> = p
                .iter()
                .map(|s| match &s.value {
                    ChoiceValue::Integer(x) => x.to_i64().map(|v| v.to_string()).unwrap_or_else(|| "big".into()),
                    ChoiceValue::Boolean(b) => if *b { "T".into() } else { "F".into() },
                    other => shape(std::slice::from_ref(&Step { addr: Vec::new(), value: other.clone() })),
                })
                .collect();
            format!("{verdict}:{}", values.join(" "))
        })
        .collect();
    listed.sort();
    println!("PATH_LIST: {}", listed.join(" | "));
}

fn wallclock_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    nanos ^ ((std::process::id() as u64) << 32)
}

fn db_settings(dbdir: String, seed: u64, phases: [Phase; 2], test_cases: u64) -> Settings {
    Settings::new()
        .database(Some(dbdir))
        .test_cases(test_cases)
        .print_blob(true)
        .seed(Some(seed))
        .phases(phases)
        .nondeterminism_strictness(NondeterminismStrictness::Quiet)
        .verbosity(Verbosity::Debug)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args[1].clone();
    let (verb, body_name) = mode.split_once('-').expect("mode is <verb>-<body>");
    let verb = verb.to_string();
    let body_name = body_name.to_string();
    let body = Body::parse(&body_name);
    if verb == "blobinfo" {
        blobinfo(body, &args[2]);
        return;
    }
    let function = body.function(&body_name);
    let test_cases = body.test_cases(&body_name);
    let arg = args[2].clone();
    let seed: Option<u64> = args.get(3).map(|s| s.parse().expect("seed is a u64"));
    let salt = match verb.as_str() {
        "discover" => 1,
        "reuse" => 2,
        _ => 3,
    };
    seed_hidden(
        seed.map(|s| s.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(salt))
            .unwrap_or_else(wallclock_seed),
    );
    let start = Instant::now();
    let key = format!("graphlive-{body_name}");
    let outcome = std::panic::catch_unwind(move || {
        let h = Hegel::new(function).__database_key(key);
        match verb.as_str() {
            "discover" => h
                .settings(db_settings(
                    arg,
                    seed.unwrap(),
                    [Phase::Generate, Phase::Shrink],
                    test_cases,
                ))
                .run(),
            "reuse" => h
                .settings(db_settings(
                    arg,
                    seed.unwrap(),
                    [Phase::Reuse, Phase::Shrink],
                    test_cases,
                ))
                .run(),
            "replay" => h
                .settings(Settings::new().database(None))
                .reproduce_failure(arg)
                .run(),
            other => panic!("unknown verb {other}"),
        }
    });
    let elapsed = start.elapsed().as_secs_f64();
    println!("SECONDS: {elapsed:.3}");
    println!("EXECUTIONS: {}", EXECUTIONS.load(Ordering::SeqCst));
    println!("FIRST_FAILURE_AT: {}", FIRST_FAILURE_AT.load(Ordering::SeqCst));
    match outcome {
        Ok(()) => println!("RESULT: PASSED"),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            println!("PANIC: {}", msg.replace('\n', " / "));
            println!("RESULT: FAILED");
        }
    }
}
