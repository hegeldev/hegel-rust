//! Shrink-quality tests for stateful state machines that use pools.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

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
