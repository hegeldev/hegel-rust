//! Shrink-quality tests for stateful state machines that use pools.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use hegel::Generator;
use hegel::generators as gs;
use hegel::stateful::{Pool, pool};
use hegel::{Hegel, Settings, TestCase};

type Trace = Arc<Mutex<Vec<String>>>;

/// Run a failing state-machine body under Hegel and return the op trace of
/// the final (shrunk) failing replay. `None` derandomizes; `Some(seed)` pins
/// the RNG seed.
fn minimal_stateful_trace<F>(run_machine: F, seed: Option<u64>) -> Vec<String>
where
    F: Fn(TestCase, Trace) + 'static,
{
    let trace: Trace = Arc::new(Mutex::new(Vec::new()));
    let trace_in_body = Arc::clone(&trace);
    let settings = match seed {
        Some(s) => Settings::new().database(None).seed(Some(s)),
        None => Settings::new().database(None).derandomize(true),
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(move |tc| {
            trace_in_body.lock().unwrap().clear();
            run_machine(tc, Arc::clone(&trace_in_body));
        })
        .settings(settings)
        .run();
    }));
    assert!(result.is_err(), "expected the state machine to fail");
    trace.lock().unwrap().clone()
}

struct DoubleIncrement {
    handles: Pool<usize>,
    counters: Vec<u32>,
    trace: Trace,
}

#[hegel::state_machine]
impl DoubleIncrement {
    #[rule]
    fn new_counter(&mut self, _tc: TestCase) {
        let id = self.counters.len();
        self.counters.push(0);
        self.handles.add(id);
        self.trace.lock().unwrap().push(format!("new v{id}"));
    }

    #[rule]
    fn increment(&mut self, tc: TestCase) {
        let id = *tc.draw(self.handles.values_reusable());
        self.counters[id] += 1;
        self.trace.lock().unwrap().push(format!("incr v{id}"));
        assert!(self.counters[id] < 2);
    }
}

#[test]
fn test_double_increment_shrinks_to_three_ops() {
    let trace = minimal_stateful_trace(
        |tc, trace| {
            let m = DoubleIncrement {
                handles: pool(&tc),
                counters: Vec::new(),
                trace,
            };
            hegel::stateful::run(m, tc);
        },
        None,
    );
    assert_eq!(trace, ["new v0", "incr v0", "incr v0"]);
}

struct DistinctPair {
    handles: Pool<usize>,
    next_id: usize,
    trace: Trace,
}

#[hegel::state_machine]
impl DistinctPair {
    #[rule]
    fn new_obj(&mut self, _tc: TestCase) {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.add(id);
        self.trace.lock().unwrap().push(format!("new v{id}"));
    }

    #[rule]
    fn pair(&mut self, tc: TestCase) {
        let a = *tc.draw(self.handles.values_reusable());
        let b = *tc.draw(self.handles.values_reusable());
        self.trace.lock().unwrap().push(format!("pair v{a} v{b}"));
        assert_eq!(a, b);
    }
}

#[test]
fn test_distinct_pair_shrinks_to_three_ops() {
    let trace = minimal_stateful_trace(
        |tc, trace| {
            let m = DistinctPair {
                handles: pool(&tc),
                next_id: 0,
                trace,
            };
            hegel::stateful::run(m, tc);
        },
        None,
    );
    assert_eq!(trace, ["new v0", "new v1", "pair v0 v1"]);
}

struct LifoClose {
    handles: Pool<usize>,
    next_id: usize,
    open: Vec<usize>,
    trace: Trace,
}

#[hegel::state_machine]
impl LifoClose {
    #[rule]
    fn open_obj(&mut self, _tc: TestCase) {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.add(id);
        self.open.push(id);
        self.trace.lock().unwrap().push(format!("open v{id}"));
    }

    #[rule]
    fn close_obj(&mut self, tc: TestCase) {
        let id = tc.draw(self.handles.values_consumed());
        self.trace.lock().unwrap().push(format!("close v{id}"));
        let last = self.open.pop().unwrap();
        assert_eq!(id, last);
    }
}

#[test]
fn test_lifo_close_shrinks_to_three_ops() {
    let trace = minimal_stateful_trace(
        |tc, trace| {
            let m = LifoClose {
                handles: pool(&tc),
                next_id: 0,
                open: Vec::new(),
                trace,
            };
            hegel::stateful::run(m, tc);
        },
        None,
    );
    assert_eq!(trace, ["open v0", "open v1", "close v0"]);
}

struct CycleGraph {
    nodes: Pool<usize>,
    next_id: usize,
    edges: Vec<(usize, usize)>,
    trace: Trace,
}

impl CycleGraph {
    fn reachable(&self, from: usize, to: usize) -> bool {
        let mut seen = vec![from];
        let mut frontier = vec![from];
        while let Some(n) = frontier.pop() {
            for &(a, b) in &self.edges {
                if a == n && !seen.contains(&b) {
                    if b == to {
                        return true;
                    }
                    seen.push(b);
                    frontier.push(b);
                }
            }
        }
        from == to
    }
}

#[hegel::state_machine]
impl CycleGraph {
    #[rule]
    fn new_node(&mut self, _tc: TestCase) {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.add(id);
        self.trace.lock().unwrap().push(format!("new v{id}"));
    }

    #[rule]
    fn add_edge(&mut self, tc: TestCase) {
        let a = *tc.draw(self.nodes.values_reusable());
        let b = *tc.draw(self.nodes.values_reusable());
        self.trace.lock().unwrap().push(format!("edge v{a} v{b}"));
        if a == b {
            return;
        }
        let cycle = self.reachable(b, a);
        self.edges.push((a, b));
        assert!(!cycle, "cycle created");
    }
}

/// The bug needs two specific nodes linked in both directions, so the shrunk
/// example must keep exactly two pool insertions and reference them stably.
/// This is the shape that stalls when pool draws are recorded as indices into
/// the live pool: deleting an unrelated insertion shifts every later
/// reference, so the deletion no longer reproduces the cycle.
#[test]
fn test_cycle_graph_shrinks_to_four_ops_across_seeds() {
    for seed in 0..10u64 {
        let trace = minimal_stateful_trace(
            |tc, trace| {
                let m = CycleGraph {
                    nodes: pool(&tc),
                    next_id: 0,
                    edges: Vec::new(),
                    trace,
                };
                hegel::stateful::run(m, tc);
            },
            Some(seed),
        );
        assert_eq!(
            trace,
            ["new v0", "new v1", "edge v0 v1", "edge v1 v0"],
            "seed {seed}"
        );
    }
}

struct NoisyTripleIncrement {
    handles: Pool<usize>,
    counters: Vec<u32>,
    scratch: u64,
    trace: Trace,
}

#[hegel::state_machine]
impl NoisyTripleIncrement {
    #[rule]
    fn new_counter(&mut self, tc: TestCase) {
        let payload = tc.draw(gs::integers::<u64>());
        let id = self.counters.len();
        self.counters.push(0);
        self.handles.add(id);
        self.scratch ^= payload;
        self.trace.lock().unwrap().push(format!("new v{id}"));
    }

    #[rule]
    fn touch(&mut self, tc: TestCase) {
        let id = *tc.draw(self.handles.values_reusable());
        self.trace.lock().unwrap().push(format!("touch v{id}"));
    }

    #[rule]
    fn scribble(&mut self, tc: TestCase) {
        let payload = tc.draw(gs::integers::<u64>());
        self.scratch = self.scratch.wrapping_add(payload);
        self.trace.lock().unwrap().push("scribble".to_string());
    }

    #[rule]
    fn increment(&mut self, tc: TestCase) {
        let id = *tc.draw(self.handles.values_reusable());
        self.counters[id] += 1;
        self.trace.lock().unwrap().push(format!("incr v{id}"));
        assert!(self.counters[id] < 3);
    }
}

#[test]
fn test_noisy_triple_increment_shrinks_to_four_ops() {
    for seed in 0..5u64 {
        let trace = minimal_stateful_trace(
            |tc, trace| {
                let m = NoisyTripleIncrement {
                    handles: pool(&tc),
                    counters: Vec::new(),
                    scratch: 0,
                    trace,
                };
                hegel::stateful::run(m, tc);
            },
            Some(seed),
        );
        assert_eq!(
            trace,
            ["new v0", "incr v0", "incr v0", "incr v0"],
            "seed {seed}"
        );
    }
}

const DOMAIN_MIN: i32 = -50;
const DOMAIN_MAX: i32 = 50;

fn domain_ranges() -> impl gs::PrintableGenerator<std::ops::Range<i32>> {
    gs::integers::<i32>()
        .min_value(DOMAIN_MIN)
        .max_value(DOMAIN_MAX - 1)
        .flat_map(|start| {
            gs::integers::<i32>()
                .min_value(start + 1)
                .max_value(DOMAIN_MAX)
                .map(move |end| start..end)
        })
}

#[derive(Clone, Debug)]
enum MapOp {
    Insert(std::ops::Range<i32>, u8),
    Remove(std::ops::Range<i32>),
}

/// The number of coalesced ranges an interval map holds after `ops`,
/// computed on a per-point model.
fn stored_ranges_after<'a>(ops: impl Iterator<Item = &'a MapOp>) -> usize {
    let mut points = std::collections::BTreeMap::new();
    for op in ops {
        match op {
            MapOp::Insert(range, value) => {
                for p in range.clone() {
                    points.insert(p, *value);
                }
            }
            MapOp::Remove(range) => {
                for p in range.clone() {
                    points.remove(&p);
                }
            }
        }
    }
    let mut runs = 0;
    let mut prev: Option<(i32, u8)> = None;
    for (&p, &v) in &points {
        if prev != Some((p - 1, v)) {
            runs += 1;
        }
        prev = Some((p, v));
    }
    runs
}

type Ops = Arc<Mutex<Vec<MapOp>>>;

/// An interval map modelled per point, with the invariant-heavy shape of a
/// real model-based suite (rangemap's): several always-true invariants plus
/// one planted failing one. Each sampled invariant adds a boolean draw to
/// every step's choice footprint.
struct IntervalMapModel {
    points: std::collections::BTreeMap<i32, u8>,
    ops: Ops,
    trace: Trace,
}

impl IntervalMapModel {
    fn stored_ranges(&self) -> usize {
        stored_ranges_after(self.ops.lock().unwrap().iter())
    }
}

#[hegel::state_machine]
impl IntervalMapModel {
    #[rule]
    fn insert(&mut self, tc: TestCase) {
        let range = tc.draw(domain_ranges());
        let value = tc.draw(gs::integers::<u8>().max_value(2));
        self.trace
            .lock()
            .unwrap()
            .push(format!("insert {range:?} v{value}"));
        self.ops
            .lock()
            .unwrap()
            .push(MapOp::Insert(range.clone(), value));
        for p in range {
            self.points.insert(p, value);
        }
    }

    #[rule]
    fn remove(&mut self, tc: TestCase) {
        let range = tc.draw(domain_ranges());
        self.trace.lock().unwrap().push(format!("remove {range:?}"));
        self.ops.lock().unwrap().push(MapOp::Remove(range.clone()));
        for p in range {
            self.points.remove(&p);
        }
    }

    #[invariant]
    fn points_in_domain(&mut self, _tc: TestCase) {
        assert!(
            self.points
                .keys()
                .all(|&p| (DOMAIN_MIN..DOMAIN_MAX).contains(&p))
        );
    }

    #[invariant]
    fn values_small(&mut self, _tc: TestCase) {
        assert!(self.points.values().all(|&v| v <= 2));
    }

    #[invariant]
    fn runs_not_more_than_points(&mut self, _tc: TestCase) {
        assert!(self.stored_ranges() <= self.points.len());
    }

    #[invariant]
    fn planted_few_ranges(&mut self, _tc: TestCase) {
        assert!(self.stored_ranges() <= 3, "map holds more than 3 ranges");
    }
}

#[test]
fn test_interval_map_shrinks_to_a_one_deletion_minimal_sequence() {
    for seed in 0..10u64 {
        let ops: Ops = Arc::new(Mutex::new(Vec::new()));
        let ops_in_body = Arc::clone(&ops);
        let trace = minimal_stateful_trace(
            move |tc, trace| {
                ops_in_body.lock().unwrap().clear();
                let m = IntervalMapModel {
                    points: std::collections::BTreeMap::new(),
                    ops: Arc::clone(&ops_in_body),
                    trace,
                };
                hegel::stateful::run(m, tc);
            },
            Some(seed),
        );
        let ops = ops.lock().unwrap().clone();
        assert!(
            stored_ranges_after(ops.iter()) > 3,
            "seed {seed}: {trace:?}"
        );
        for skip in 0..ops.len() {
            let without = ops
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != skip)
                .map(|(_, op)| op);
            assert!(
                stored_ranges_after(without) <= 3,
                "seed {seed}: step {} is redundant in {trace:?}",
                skip + 1,
            );
        }
    }
}

/// `DoubleIncrement` above, plus enough always-true invariants that a step's
/// choice footprint (stop draw + rule selection + pool draw + one sampled
/// boolean per invariant) exceeds the shrinker's largest deletion window.
struct InvariantHeavyDoubleIncrement {
    handles: Pool<usize>,
    counters: Vec<u32>,
    trace: Trace,
}

#[hegel::state_machine]
impl InvariantHeavyDoubleIncrement {
    #[rule]
    fn new_counter(&mut self, _tc: TestCase) {
        let id = self.counters.len();
        self.counters.push(0);
        self.handles.add(id);
        self.trace.lock().unwrap().push(format!("new v{id}"));
    }

    #[rule]
    fn increment(&mut self, tc: TestCase) {
        let id = *tc.draw(self.handles.values_reusable());
        self.counters[id] += 1;
        self.trace.lock().unwrap().push(format!("incr v{id}"));
    }

    #[invariant]
    fn ids_dense(&mut self, _tc: TestCase) {
        assert!(self.handles.len() == self.counters.len());
    }

    #[invariant]
    fn counts_fit(&mut self, _tc: TestCase) {
        assert!(self.counters.iter().all(|&c| c < 100));
    }

    #[invariant]
    fn sum_fits(&mut self, _tc: TestCase) {
        assert!(self.counters.iter().sum::<u32>() < 10_000);
    }

    #[invariant]
    fn non_negative_len(&mut self, _tc: TestCase) {
        assert!(self.counters.capacity() >= self.counters.len());
    }

    #[invariant]
    fn handles_populated(&mut self, _tc: TestCase) {
        assert!(self.handles.len() <= self.counters.len());
    }

    #[invariant]
    fn planted_below_two(&mut self, _tc: TestCase) {
        assert!(self.counters.iter().all(|&c| c < 2), "counter reached 2");
    }
}

#[test]
fn test_invariant_heavy_double_increment_shrinks_to_three_ops() {
    for seed in 0..5u64 {
        let trace = minimal_stateful_trace(
            |tc, trace| {
                let m = InvariantHeavyDoubleIncrement {
                    handles: pool(&tc),
                    counters: Vec::new(),
                    trace,
                };
                hegel::stateful::run(m, tc);
            },
            Some(seed),
        );
        assert_eq!(trace, ["new v0", "incr v0", "incr v0"], "seed {seed}");
    }
}
