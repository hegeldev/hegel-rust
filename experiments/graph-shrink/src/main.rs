//! Experiment 018: shrinking the counterexample as a graph.
//! Spec and results: notes/experiments/018-graph-shrink/notes.md
//!
//! For each body and confirmation-batch size, a trial discovers a failing
//! run, captures the failing runs of a confirmation batch (as 017 does) and
//! builds three starting graphs from them: the single discovery run, the
//! exact merge and the compatible merge. Each start is then shrunk by a
//! greedy shrinker whose moves are graph edits — delete an edge, contract a
//! node into a successor, merge two unrelated nodes, shrink an edge's value
//! — every candidate judged by replaying the candidate graph through the
//! engine. Three judges: strict (every replay fails without leaving the
//! graph and ends on a terminal node), lenient (every replay fails, rescue
//! allowed) and learn (strict, and the realized run of a failing replay
//! that left the graph is grafted into the current graph when the graph
//! cannot walk a run of its shape). The shrunk graphs are measured for
//! size, wrong paths and cold reproduction.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use hegel_c::__bench::{
    replay_case, BigInt, ChoiceValue, DataSource, Divergence, ExternalReplay, Failure, ReplayKind,
    ReplayOutcome, TestCaseResult, ToPrimitive,
};

const EXTEND: usize = 4;
const CONFIRMS: [usize; 2] = [20, 100];
const DISCOVERY_CAP: u64 = 100_000;
const PATH_CAP: usize = 200_000;
const EXEC_CAP: u64 = 100_000;
const PASS_CAP: usize = 20;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1))
    }

    fn f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Block,
    Shift,
    Sum,
}

#[derive(Clone, Copy, Debug)]
struct Body {
    kind: Kind,
    k: usize,
}

impl Body {
    fn name(&self) -> String {
        let kind = match self.kind {
            Kind::Block => "block",
            Kind::Shift => "shift",
            Kind::Sum => "sum",
        };
        format!("{kind}{}", self.k)
    }

    fn budget(&self) -> usize {
        1 + 2 * self.k + EXTEND
    }

    /// The smallest graph that reproduces the failure on every hidden coin:
    /// a chain of k diamonds, each with a bool arm and an int arm.
    fn ideal(&self) -> (usize, usize) {
        match self.kind {
            Kind::Shift => (2 + 2 * self.k, 1 + 3 * self.k),
            _ => (2 + self.k, 1 + 2 * self.k),
        }
    }
}

fn parse_body(name: &str) -> Option<Body> {
    let idx = name.find(|c: char| c.is_ascii_digit())?;
    let k: usize = name[idx..].parse().ok()?;
    let kind = match &name[..idx] {
        "block" => Kind::Block,
        "shift" => Kind::Shift,
        "sum" => Kind::Sum,
        _ => return None,
    };
    Some(Body { kind, k })
}

fn body_draws(body: Body, hidden: &mut Rng, ds: &dyn DataSource) -> Result<bool, ()> {
    let int = |lo: i64, hi: i64| -> Result<i64, ()> {
        ds.generate_integer(&BigInt::from(lo), &BigInt::from(hi))
            .map(|v| v.to_i64().unwrap())
            .map_err(|_| ())
    };
    let coin = || ds.generate_boolean(0.5, None).map_err(|_| ());
    let a = coin()?;
    let mut hot = true;
    let mut sum = 0i64;
    for _ in 0..body.k {
        if hidden.f64() < 0.5 {
            let b = coin()?;
            hot &= b;
            sum += if b { 100 } else { 0 };
        } else {
            let x = int(0, 100)?;
            if body.kind == Kind::Shift {
                coin()?;
            }
            hot &= x >= 60;
            sum += x;
        }
    }
    Ok(match body.kind {
        Kind::Sum => a && sum >= 60 * body.k as i64,
        _ => a && hot,
    })
}

/// The body's verdict on a whole sequence, read off its shape: `Some(fail)`
/// for a well-formed sequence, `None` for one no run of the body produces.
fn predicate(body: Body, path: &[ChoiceValue]) -> Option<bool> {
    let mut it = path.iter();
    let a = match it.next()? {
        ChoiceValue::Boolean(a) => *a,
        _ => return None,
    };
    let mut hot = true;
    let mut sum = 0i64;
    for _ in 0..body.k {
        match it.next()? {
            ChoiceValue::Boolean(b) => {
                hot &= *b;
                sum += if *b { 100 } else { 0 };
            }
            ChoiceValue::Integer(x) => {
                let x = x.to_i64()?;
                if body.kind == Kind::Shift {
                    match it.next()? {
                        ChoiceValue::Boolean(_) => {}
                        _ => return None,
                    }
                }
                hot &= x >= 60;
                sum += x;
            }
            _ => return None,
        }
    }
    if it.next().is_some() {
        return None;
    }
    Some(match body.kind {
        Kind::Sum => a && sum >= 60 * body.k as i64,
        _ => a && hot,
    })
}

fn run(body: Body, hidden: &RefCell<Rng>, kind: ReplayKind<'_>, seed: u64) -> ReplayOutcome {
    replay_case(kind, seed, |ds| {
        let verdict = body_draws(body, &mut hidden.borrow_mut(), ds.as_ref());
        let result = match verdict {
            Err(()) => TestCaseResult::Overrun,
            Ok(true) => TestCaseResult::Interesting(Failure {
                origin: "bug".to_string(),
                reproduce_blob: None,
                caveat: None,
            }),
            Ok(false) => TestCaseResult::Valid,
        };
        ds.mark_complete(&result);
    })
    .unwrap()
}

fn shape(values: &[ChoiceValue]) -> String {
    values
        .iter()
        .map(|v| match v {
            ChoiceValue::Integer(_) => 'i',
            ChoiceValue::Boolean(_) => 'b',
            ChoiceValue::Float(_) => 'f',
            ChoiceValue::Bytes(_) => 'y',
            ChoiceValue::String(_) => 's',
            ChoiceValue::Clone(_) => 'c',
        })
        .collect()
}

fn kind_of(v: &ChoiceValue) -> String {
    shape(std::slice::from_ref(v))
}

fn value_key(v: &ChoiceValue) -> String {
    match v {
        ChoiceValue::Integer(n) => format!("i{n}"),
        ChoiceValue::Boolean(b) => format!("b{b}"),
        other => format!("{:?}", kind_of(other)),
    }
}

/// The shrink order on one value: booleans before integers, false before
/// true, integers by magnitude.
fn value_rank(v: &ChoiceValue) -> (u8, u64) {
    match v {
        ChoiceValue::Boolean(b) => (0, *b as u64),
        ChoiceValue::Integer(n) => (1, n.to_i64().map(|x| x.unsigned_abs()).unwrap_or(u64::MAX)),
        _ => (2, 0),
    }
}

type Key = (usize, usize, Vec<(u8, u64)>);

#[derive(Default, Clone, Debug)]
struct Node {
    edges: Vec<(ChoiceValue, usize)>,
    terminal: bool,
}

#[derive(Clone, Debug)]
struct Graph {
    nodes: Vec<Node>,
    root: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Merge {
    Exact,
    Compatible,
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

impl Graph {
    fn new() -> Self {
        Graph {
            nodes: vec![Node::default()],
            root: 0,
        }
    }

    fn insert(&mut self, run: &[ChoiceValue]) {
        let mut n = self.root;
        for v in run {
            let next = match self.nodes[n].edges.iter().find(|(e, _)| e == v) {
                Some(&(_, t)) => t,
                None => {
                    let t = self.nodes.len();
                    self.nodes.push(Node::default());
                    self.nodes[n].edges.push((v.clone(), t));
                    t
                }
            };
            n = next;
        }
        self.nodes[n].terminal = true;
    }

    fn edge_count(&self) -> usize {
        self.bfs_order()
            .iter()
            .map(|&n| self.nodes[n].edges.len())
            .sum()
    }

    fn post_order(&self) -> Vec<usize> {
        let mut order = Vec::new();
        let mut seen = vec![false; self.nodes.len()];
        fn visit(g: &Graph, n: usize, seen: &mut [bool], order: &mut Vec<usize>) {
            if seen[n] {
                return;
            }
            seen[n] = true;
            for &(_, c) in &g.nodes[n].edges {
                visit(g, c, seen, order);
            }
            order.push(n);
        }
        visit(self, self.root, &mut seen, &mut order);
        order
    }

    /// Merge nodes with identical futures, value for value.
    fn merged_exact(&self) -> Graph {
        let order = self.post_order();
        let mut sig_of: Vec<usize> = vec![usize::MAX; self.nodes.len()];
        let mut intern: HashMap<String, usize> = HashMap::new();
        for &n in &order {
            let node = &self.nodes[n];
            let items: Vec<String> = node
                .edges
                .iter()
                .map(|(v, c)| format!("{}={}", value_key(v), sig_of[*c]))
                .collect();
            let key = format!("{}|{}", node.terminal, items.join(","));
            let next = intern.len();
            sig_of[n] = *intern.entry(key).or_insert(next);
        }
        let mut merged = Graph {
            nodes: Vec::new(),
            root: 0,
        };
        let mut canon: HashMap<usize, usize> = HashMap::new();
        let mut map: Vec<usize> = vec![usize::MAX; self.nodes.len()];
        for &n in &order {
            let m = *canon.entry(sig_of[n]).or_insert_with(|| {
                merged.nodes.push(Node {
                    edges: Vec::new(),
                    terminal: self.nodes[n].terminal,
                });
                merged.nodes.len() - 1
            });
            for (v, c) in &self.nodes[n].edges {
                let target = map[*c];
                if !merged.nodes[m].edges.iter().any(|(e, _)| e == v) {
                    merged.nodes[m].edges.push((v.clone(), target));
                }
            }
            map[n] = m;
        }
        merged.root = map[self.root];
        merged
    }

    fn bfs_order(&self) -> Vec<usize> {
        let mut order = vec![self.root];
        let mut seen = vec![false; self.nodes.len()];
        seen[self.root] = true;
        let mut i = 0;
        while i < order.len() {
            let n = order[i];
            i += 1;
            for &(_, c) in &self.nodes[n].edges {
                if !seen[c] {
                    seen[c] = true;
                    order.push(c);
                }
            }
        }
        order
    }

    fn compatible(
        &self,
        parent: &mut [usize],
        a: usize,
        b: usize,
        assumed: &mut Vec<(usize, usize)>,
    ) -> bool {
        let (a, b) = (find(parent, a), find(parent, b));
        if a == b || assumed.contains(&(a, b)) {
            return true;
        }
        if self.nodes[a].terminal != self.nodes[b].terminal {
            return false;
        }
        assumed.push((a, b));
        let edges_b = self.nodes[b].edges.clone();
        for (v, c) in edges_b {
            let kind = kind_of(&v);
            let targets: Vec<usize> = self.nodes[a]
                .edges
                .iter()
                .filter(|(e, _)| kind_of(e) == kind)
                .map(|&(_, t)| t)
                .collect();
            for t in targets {
                if !self.compatible(parent, t, c, assumed) {
                    return false;
                }
            }
        }
        true
    }

    fn reaches_via(&self, parent: &mut [usize], from: usize, to: usize) -> bool {
        let to = find(parent, to);
        let mut stack = vec![find(parent, from)];
        let mut seen = vec![false; self.nodes.len()];
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if seen[n] {
                continue;
            }
            seen[n] = true;
            for i in 0..self.nodes[n].edges.len() {
                let c = find(parent, self.nodes[n].edges[i].1);
                stack.push(c);
            }
        }
        false
    }

    fn related_via(&self, parent: &mut [usize], a: usize, b: usize) -> bool {
        self.reaches_via(parent, a, b) || self.reaches_via(parent, b, a)
    }

    fn fold(&mut self, parent: &mut [usize], a: usize, b: usize) {
        let (a, b) = (find(parent, a), find(parent, b));
        if a == b {
            return;
        }
        parent[b] = a;
        let edges_b = std::mem::take(&mut self.nodes[b].edges);
        for (v, c) in edges_b {
            match self.nodes[a]
                .edges
                .iter()
                .find(|(e, _)| *e == v)
                .map(|&(_, t)| t)
            {
                Some(t) if find(parent, t) == find(parent, c) => {}
                Some(t) if self.related_via(parent, t, c) => self.nodes[a].edges.push((v, c)),
                Some(t) => self.fold(parent, t, c),
                None => self.nodes[a].edges.push((v, c)),
            }
        }
    }

    fn is_acyclic(&self) -> bool {
        let mut state = vec![0u8; self.nodes.len()];
        fn visit(g: &Graph, n: usize, state: &mut [u8]) -> bool {
            match state[n] {
                1 => return false,
                2 => return true,
                _ => {}
            }
            state[n] = 1;
            for &(_, c) in &g.nodes[n].edges {
                if !visit(g, c, state) {
                    return false;
                }
            }
            state[n] = 2;
            true
        }
        visit(self, self.root, &mut state)
    }

    /// Greedy state merging in the manner of RPNI (017's `compat`).
    fn merged_compatible(&self) -> Graph {
        let mut g = self.clone();
        let mut parent: Vec<usize> = (0..g.nodes.len()).collect();
        let mut accepted: Vec<usize> = Vec::new();
        for n in self.bfs_order() {
            if find(&mut parent, n) != n {
                continue;
            }
            let mut folded = false;
            for i in 0..accepted.len() {
                let a = accepted[i];
                if find(&mut parent, a) != a {
                    continue;
                }
                let mut assumed = Vec::new();
                if !g.related_via(&mut parent, a, n)
                    && g.compatible(&mut parent, a, n, &mut assumed)
                {
                    g.fold(&mut parent, a, n);
                    folded = true;
                    break;
                }
            }
            if !folded {
                accepted.push(n);
            }
        }
        let mut ids: HashMap<usize, usize> = HashMap::new();
        let mut merged = Graph {
            nodes: Vec::new(),
            root: 0,
        };
        let mut queue = vec![find(&mut parent, self.root)];
        ids.insert(queue[0], 0);
        merged.nodes.push(Node {
            edges: Vec::new(),
            terminal: g.nodes[queue[0]].terminal,
        });
        let mut i = 0;
        while i < queue.len() {
            let n = queue[i];
            i += 1;
            let edges = g.nodes[n].edges.clone();
            for (v, c) in edges {
                let c = find(&mut parent, c);
                let next_id = ids.len();
                let id = *ids.entry(c).or_insert_with(|| {
                    merged.nodes.push(Node {
                        edges: Vec::new(),
                        terminal: g.nodes[c].terminal,
                    });
                    queue.push(c);
                    next_id
                });
                let from = ids[&n];
                if !merged.nodes[from].edges.iter().any(|(e, _)| *e == v) {
                    merged.nodes[from].edges.push((v, id));
                }
            }
        }
        assert!(merged.is_acyclic(), "compatible merge closed a cycle");
        merged
    }

    fn merge(&self, mode: Merge) -> Graph {
        match mode {
            Merge::Compatible => self.merged_compatible(),
            Merge::Exact => self.merged_exact(),
        }
    }

    /// The states reachable by a path of each length.
    fn at_depth(&self, max_depth: usize) -> Vec<Vec<usize>> {
        let mut levels: Vec<Vec<usize>> = vec![vec![self.root]];
        for d in 0..max_depth {
            let mut next: Vec<usize> = Vec::new();
            for &n in &levels[d] {
                for &(_, c) in &self.nodes[n].edges {
                    if !next.contains(&c) {
                        next.push(c);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            levels.push(next);
        }
        levels
    }

    fn depth(&self) -> usize {
        let mut memo: Vec<Option<usize>> = vec![None; self.nodes.len()];
        fn d(g: &Graph, n: usize, memo: &mut [Option<usize>]) -> usize {
            if let Some(v) = memo[n] {
                return v;
            }
            let v = g.nodes[n]
                .edges
                .iter()
                .map(|&(_, c)| 1 + d(g, c, memo))
                .max()
                .unwrap_or(0);
            memo[n] = Some(v);
            v
        }
        d(self, self.root, &mut memo)
    }

    fn count_paths(&self) -> u64 {
        let mut memo: Vec<Option<u64>> = vec![None; self.nodes.len()];
        fn c(g: &Graph, n: usize, memo: &mut [Option<u64>]) -> u64 {
            if let Some(v) = memo[n] {
                return v;
            }
            let node = &g.nodes[n];
            let mut total: u64 = if node.terminal { 1 } else { 0 };
            for &(_, ch) in &node.edges {
                total = total.saturating_add(c(g, ch, memo));
            }
            memo[n] = Some(total);
            total
        }
        c(self, self.root, &mut memo)
    }

    fn paths(&self, cap: usize) -> Vec<Vec<ChoiceValue>> {
        let mut out = Vec::new();
        let mut stack: Vec<ChoiceValue> = Vec::new();
        fn walk(
            g: &Graph,
            n: usize,
            stack: &mut Vec<ChoiceValue>,
            out: &mut Vec<Vec<ChoiceValue>>,
            cap: usize,
        ) {
            if out.len() >= cap {
                return;
            }
            let node = &g.nodes[n];
            if node.terminal {
                out.push(stack.clone());
            }
            for (v, c) in &node.edges {
                stack.push(v.clone());
                walk(g, *c, stack, out, cap);
                stack.pop();
            }
        }
        walk(self, self.root, &mut stack, &mut out, cap);
        out
    }

    /// The shrink order: fewer edges, then fewer nodes, then the edge
    /// values in breadth-first order, each by `value_rank`.
    fn key(&self) -> Key {
        let order = self.bfs_order();
        let mut values = Vec::new();
        for &n in &order {
            for (v, _) in &self.nodes[n].edges {
                values.push(value_rank(v));
            }
        }
        (values.len(), order.len(), values)
    }

    /// Reachable nodes renumbered in breadth-first order.
    fn normalized(&self) -> Graph {
        let order = self.bfs_order();
        let mut map = vec![usize::MAX; self.nodes.len()];
        for (i, &n) in order.iter().enumerate() {
            map[n] = i;
        }
        let nodes = order
            .iter()
            .map(|&n| Node {
                edges: self.nodes[n]
                    .edges
                    .iter()
                    .map(|(v, c)| (v.clone(), map[*c]))
                    .collect(),
                terminal: self.nodes[n].terminal,
            })
            .collect();
        Graph { nodes, root: 0 }
    }

    fn reachable(&self) -> Vec<bool> {
        let mut seen = vec![false; self.nodes.len()];
        for n in self.bfs_order() {
            seen[n] = true;
        }
        seen
    }

    fn reaches(&self, from: usize, to: usize) -> bool {
        let mut parent: Vec<usize> = (0..self.nodes.len()).collect();
        self.reaches_via(&mut parent, from, to)
    }

    /// The edges into `n`, as (source node, value): what a walk serves when
    /// it takes one of them.
    fn edges_into(&self, n: usize) -> Vec<(usize, ChoiceValue)> {
        let mut into = Vec::new();
        for p in self.bfs_order() {
            for (v, t) in &self.nodes[p].edges {
                if *t == n {
                    into.push((p, v.clone()));
                }
            }
        }
        into
    }

    fn delete_edge(&self, n: usize, i: usize) -> Graph {
        let mut g = self.clone();
        g.nodes[n].edges.remove(i);
        g
    }

    /// Redirect every edge into `n` to its successor `t` (dropping a redirected
    /// edge whose value `p` already serves), deleting `n` and its edges.
    fn contract(&self, n: usize, t: usize) -> Graph {
        let mut g = self.clone();
        for p in 0..g.nodes.len() {
            let mut kept: Vec<(ChoiceValue, usize)> = Vec::new();
            for (v, c) in std::mem::take(&mut g.nodes[p].edges) {
                let c = if c == n { t } else { c };
                if c == t && kept.iter().any(|(e, _)| *e == v) {
                    continue;
                }
                kept.push((v, c));
            }
            g.nodes[p].edges = kept;
        }
        g.nodes[n].edges.clear();
        if n == g.root {
            g.root = t;
        }
        g
    }

    /// Fold `b` into `a`: `a` takes `b`'s edges after its own (an identical
    /// edge is not repeated) and every edge into `b` is redirected to `a`.
    fn merge_nodes(&self, a: usize, b: usize) -> Graph {
        let mut g = self.clone();
        let edges_b = std::mem::take(&mut g.nodes[b].edges);
        for e in edges_b {
            if !g.nodes[a].edges.contains(&e) {
                g.nodes[a].edges.push(e);
            }
        }
        for p in 0..g.nodes.len() {
            for e in g.nodes[p].edges.iter_mut() {
                if e.1 == b {
                    e.1 = a;
                }
            }
        }
        if b == g.root {
            g.root = a;
        }
        g
    }

    fn set_value(&self, n: usize, i: usize, v: ChoiceValue) -> Graph {
        let mut g = self.clone();
        g.nodes[n].edges[i].0 = v;
        g
    }

    /// Whether the first-fit walk serves a run of this shape (the kinds of
    /// its draws, in order) without a misfit and ends on a terminal node.
    fn walks_shape(&self, run: &[ChoiceValue]) -> bool {
        let mut n = self.root;
        for v in run {
            let kind = kind_of(v);
            match self.nodes[n].edges.iter().find(|(e, _)| kind_of(e) == kind) {
                Some(&(_, t)) => n = t,
                None => return false,
            }
        }
        self.nodes[n].terminal
    }

    /// Whether `suffix` is a path from `n` ending on a terminal node.
    fn has_path(&self, mut n: usize, suffix: &[ChoiceValue]) -> bool {
        for v in suffix {
            match self.nodes[n].edges.iter().find(|(e, _)| e == v) {
                Some(&(_, t)) => n = t,
                None => return false,
            }
        }
        self.nodes[n].terminal
    }

    /// Learn a failing run the graph does not contain: follow it while it
    /// fits; where it departs, link to a state at the same depth from which
    /// the rest of the run is already a path (the rejoin the rescue made,
    /// made permanent), and only where no such state exists append the
    /// rest as new nodes. Returns the number of edges linked.
    fn graft(&mut self, run: &[ChoiceValue], max_depth: usize) -> usize {
        let levels = self.at_depth(max_depth);
        let mut n = self.root;
        let mut linked = 0;
        for (i, v) in run.iter().enumerate() {
            if let Some(&(_, t)) = self.nodes[n].edges.iter().find(|(e, _)| e == v) {
                n = t;
                continue;
            }
            let donor = levels.get(i).and_then(|level| {
                level.iter().find_map(|&m| {
                    self.nodes[m]
                        .edges
                        .iter()
                        .find(|(e, t)| {
                            e == v && self.has_path(*t, &run[i + 1..]) && !self.reaches(*t, n)
                        })
                        .map(|&(_, t)| t)
                })
            });
            match donor {
                Some(t) => {
                    self.nodes[n].edges.push((v.clone(), t));
                    linked += 1;
                    return linked;
                }
                None => {
                    let t = self.nodes.len();
                    self.nodes.push(Node::default());
                    self.nodes[n].edges.push((v.clone(), t));
                    n = t;
                }
            }
        }
        if std::env::var("TRACE_GRAFT").is_ok() && !self.nodes[n].terminal {
            eprintln!(
                "  graft marks {n} terminal after a run of {} values: {}",
                run.len(),
                run.iter().map(value_key).collect::<Vec<_>>().join(" ")
            );
        }
        self.nodes[n].terminal = true;
        linked
    }
}

/// What a walk reports back: the node it ended on and the edges it served
/// while anchored, as (node, value).
#[derive(Default)]
struct Trace {
    node: Option<usize>,
    served: Vec<(usize, ChoiceValue)>,
}

struct Walk {
    graph: Arc<Graph>,
    at_depth: Arc<Vec<Vec<usize>>>,
    node: Option<usize>,
    trace: Arc<Mutex<Trace>>,
    divergence: Option<Divergence>,
    longest: usize,
}

impl Walk {
    fn new(graph: Arc<Graph>, at_depth: Arc<Vec<Vec<usize>>>, trace: Arc<Mutex<Trace>>) -> Self {
        let longest = graph.depth();
        trace.lock().unwrap().node = Some(graph.root);
        Walk {
            node: Some(graph.root),
            graph,
            at_depth,
            trace,
            divergence: None,
            longest,
        }
    }

    fn donor(
        &self,
        position: usize,
        fits: &dyn Fn(&ChoiceValue) -> bool,
    ) -> Option<(ChoiceValue, usize)> {
        let level = self.at_depth.get(position)?;
        level.iter().find_map(|&n| {
            self.graph.nodes[n]
                .edges
                .iter()
                .find(|(v, _)| fits(v))
                .map(|(v, t)| (v.clone(), *t))
        })
    }
}

/// The graph walk with the rejoin rescue of 017: on a misfit, serve from the
/// first state at the same depth that has a fitting edge and re-anchor there.
impl ExternalReplay for Walk {
    fn resolve(
        &mut self,
        stream: &[usize],
        position: usize,
        fits: &dyn Fn(&ChoiceValue) -> bool,
    ) -> Option<ChoiceValue> {
        if !stream.is_empty() {
            return None;
        }
        if let Some(n) = self.node {
            let node = &self.graph.nodes[n];
            if let Some((v, t)) = node.edges.iter().find(|(v, _)| fits(v)) {
                self.node = Some(*t);
                let mut trace = self.trace.lock().unwrap();
                trace.node = self.node;
                trace.served.push((n, v.clone()));
                return Some(v.clone());
            }
            if self.divergence.is_none() {
                self.divergence = Some(Divergence {
                    stream: Vec::new(),
                    position,
                });
            }
            self.node = None;
        }
        let served = self.donor(position, fits);
        self.node = served.as_ref().map(|&(_, t)| t);
        self.trace.lock().unwrap().node = self.node;
        served.map(|(v, _)| v)
    }

    fn divergence(&self) -> Option<Divergence> {
        self.divergence.clone()
    }

    fn longest(&self) -> usize {
        self.longest
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Judge {
    Strict,
    Lenient,
    Learn,
}

impl Judge {
    fn name(self) -> &'static str {
        match self {
            Judge::Strict => "strict",
            Judge::Lenient => "lenient",
            Judge::Learn => "learn",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Move {
    Delete,
    Contract,
    MergeNodes,
    Value,
}

const MOVES: [Move; 4] = [Move::Delete, Move::Contract, Move::MergeNodes, Move::Value];

impl Move {
    fn name(self) -> &'static str {
        match self {
            Move::Delete => "delete",
            Move::Contract => "contract",
            Move::MergeNodes => "merge",
            Move::Value => "value",
        }
    }
}

#[derive(Default, Debug, Clone)]
struct ShrinkStats {
    execs: u64,
    passes: usize,
    accepts: [u64; 4],
    rejects: [u64; 4],
    free: u64,
    learned: u64,
    warmed: u64,
    linked: u64,
    redundant: u64,
    unexercised: u64,
    start_ok: bool,
    max_edges: usize,
    max_nodes: usize,
    capped: bool,
}

struct Replayed {
    outcome: ReplayOutcome,
    clean: bool,
    served: Vec<(usize, ChoiceValue)>,
}

struct Shrinker<'a> {
    body: Body,
    hidden: &'a RefCell<Rng>,
    seed: &'a mut dyn FnMut() -> u64,
    k: usize,
    judge: Judge,
    warmup: usize,
    stats: ShrinkStats,
}

impl<'a> Shrinker<'a> {
    fn replay(&mut self, graph: &Arc<Graph>, at_depth: &Arc<Vec<Vec<usize>>>) -> Replayed {
        self.stats.execs += 1;
        let trace = Arc::new(Mutex::new(Trace::default()));
        let walk = Walk::new(Arc::clone(graph), Arc::clone(at_depth), Arc::clone(&trace));
        let budget = self.body.budget().max(walk.longest + EXTEND);
        let outcome = run(
            self.body,
            self.hidden,
            ReplayKind::External {
                resolver: Box::new(walk),
                budget,
            },
            (self.seed)(),
        );
        let trace = std::mem::take(&mut *trace.lock().unwrap());
        let clean =
            outcome.divergence.is_none() && trace.node.is_some_and(|n| graph.nodes[n].terminal);
        Replayed {
            outcome,
            clean,
            served: trace.served,
        }
    }

    /// Judge a candidate: `k` replays, stopping at the first that does not
    /// count as a failure — under `Strict` and `Learn` a failing replay that
    /// left the graph or ended off a terminal node is not one. Returns the
    /// verdict and, under `Learn`, the realized runs of failing replays the
    /// graph did not contain. A candidate whose edited edge (`watch`) no
    /// replay served is not accepted: the edit was never tested.
    fn accept(
        &mut self,
        cand: &Graph,
        watch: &[(usize, ChoiceValue)],
    ) -> (bool, Vec<Vec<ChoiceValue>>) {
        let graph = Arc::new(cand.clone());
        let at_depth = Arc::new(graph.at_depth(self.body.budget()));
        let mut learned = Vec::new();
        let mut exercised = watch.is_empty();
        for _ in 0..self.k {
            let r = self.replay(&graph, &at_depth);
            if !r.outcome.interesting {
                return (false, learned);
            }
            exercised |= watch.iter().any(|w| r.served.contains(w));
            if r.clean {
                continue;
            }
            match self.judge {
                Judge::Strict => return (false, learned),
                Judge::Learn => {
                    learned.push(r.outcome.realized);
                    return (false, learned);
                }
                Judge::Lenient => {}
            }
        }
        if !exercised {
            self.stats.unexercised += 1;
        }
        (exercised, learned)
    }

    fn track(&mut self, g: &Graph) {
        self.stats.max_edges = self.stats.max_edges.max(g.edge_count());
        self.stats.max_nodes = self.stats.max_nodes.max(g.bfs_order().len());
    }

    fn consider(
        &mut self,
        g: &mut Graph,
        cand: Graph,
        mv: Move,
        watch: Vec<(usize, ChoiceValue)>,
    ) -> bool {
        if cand.key() >= g.key() {
            return false;
        }
        let (ok, learned) = self.accept(&cand, &watch);
        if ok {
            if std::env::var("TRACE_WRONG").is_ok() {
                let wrong = |x: &Graph| {
                    x.paths(PATH_CAP)
                        .iter()
                        .filter(|p| predicate(self.body, p) != Some(true))
                        .count()
                };
                let (before, after) = (wrong(g), wrong(&cand));
                if after > before {
                    eprintln!(
                        "  {:?} raised wrong paths {before} -> {after}: {}",
                        mv,
                        cand.paths(PATH_CAP)
                            .iter()
                            .filter(|p| predicate(self.body, p) != Some(true))
                            .take(2)
                            .map(|p| p.iter().map(value_key).collect::<Vec<_>>().join(" "))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    );
                }
            }
            *g = cand;
            self.stats.accepts[mv as usize] += 1;
        } else {
            self.stats.rejects[mv as usize] += 1;
        }
        for run_values in learned {
            if g.walks_shape(&run_values) {
                self.stats.redundant += 1;
                continue;
            }
            self.stats.linked += g.graft(&run_values, self.body.budget()) as u64;
            self.stats.learned += 1;
        }
        self.track(g);
        ok
    }

    fn over(&self) -> bool {
        self.stats.execs >= EXEC_CAP
    }

    /// Under `Learn`, replay the current graph `k` times without stopping
    /// early and graft every failing run it did not contain, repeating while
    /// a round grafts something (at most `warmup` rounds). Returns whether
    /// anything was grafted.
    fn warm_up(&mut self, g: &mut Graph) -> bool {
        if !matches!(self.judge, Judge::Learn) {
            return false;
        }
        let mut grafted = false;
        for _ in 0..self.warmup {
            if self.over() {
                break;
            }
            let graph = Arc::new(g.clone());
            let at_depth = Arc::new(graph.at_depth(self.body.budget()));
            let mut runs = Vec::new();
            for _ in 0..self.k {
                let r = self.replay(&graph, &at_depth);
                if r.outcome.interesting && !r.clean {
                    runs.push(r.outcome.realized);
                }
            }
            let mut any = false;
            for run_values in runs {
                if g.walks_shape(&run_values) {
                    self.stats.redundant += 1;
                    continue;
                }
                self.stats.linked += g.graft(&run_values, self.body.budget()) as u64;
                self.stats.learned += 1;
                self.stats.warmed += 1;
                any = true;
            }
            if !any {
                break;
            }
            grafted = true;
            *g = g.normalized();
            self.track(g);
        }
        grafted
    }

    fn pass_delete(&mut self, g: &mut Graph) {
        let mut n = 0;
        while n < g.nodes.len() {
            let mut i = 0;
            while i < g.nodes[n].edges.len() {
                if self.over() {
                    return;
                }
                if !g.reachable()[n] {
                    break;
                }
                let value = g.nodes[n].edges[i].0.clone();
                let dead = g.nodes[n].edges[..i].iter().any(|(v, _)| *v == value);
                if dead {
                    *g = g.delete_edge(n, i);
                    self.stats.free += 1;
                    continue;
                }
                let keeps_a_way_on = g.nodes[n].edges.len() > 1 || g.nodes[n].terminal;
                if keeps_a_way_on && self.consider(g, g.delete_edge(n, i), Move::Delete, Vec::new())
                {
                    continue;
                }
                i += 1;
            }
            n += 1;
        }
    }

    fn pass_contract(&mut self, g: &mut Graph) {
        for n in g.bfs_order() {
            if self.over() {
                return;
            }
            if !g.reachable()[n] {
                continue;
            }
            let mut targets: Vec<usize> = g.nodes[n].edges.iter().map(|&(_, t)| t).collect();
            targets.dedup();
            let into = g.edges_into(n);
            for t in targets {
                if self.consider(g, g.contract(n, t), Move::Contract, into.clone()) {
                    break;
                }
            }
        }
    }

    fn pass_merge(&mut self, g: &mut Graph) {
        let order = g.bfs_order();
        let levels = g.at_depth(self.body.budget());
        let mut depths: Vec<Vec<usize>> = vec![Vec::new(); g.nodes.len()];
        for (d, level) in levels.iter().enumerate() {
            for &n in level {
                depths[n].push(d);
            }
        }
        for bi in 1..order.len() {
            let b = order[bi];
            for &a in &order[..bi] {
                if self.over() {
                    return;
                }
                let alive = g.reachable();
                if !alive[a] || !alive[b] {
                    break;
                }
                if g.nodes[a].terminal != g.nodes[b].terminal
                    || depths[a] != depths[b]
                    || g.reaches(a, b)
                    || g.reaches(b, a)
                {
                    continue;
                }
                let into = g.edges_into(b);
                if self.consider(g, g.merge_nodes(a, b), Move::MergeNodes, into) {
                    break;
                }
            }
        }
    }

    fn pass_values(&mut self, g: &mut Graph) {
        for n in g.bfs_order() {
            let mut i = 0;
            while i < g.nodes[n].edges.len() {
                if self.over() {
                    return;
                }
                if !g.reachable()[n] {
                    break;
                }
                match g.nodes[n].edges[i].0.clone() {
                    ChoiceValue::Boolean(true) => {
                        let v = ChoiceValue::Boolean(false);
                        self.consider(g, g.set_value(n, i, v.clone()), Move::Value, vec![(n, v)]);
                    }
                    ChoiceValue::Integer(x) => {
                        let x = x.to_i64().unwrap();
                        if x > 0 {
                            let int = |v: i64| ChoiceValue::Integer(BigInt::from(v));
                            if !self.consider(
                                g,
                                g.set_value(n, i, int(0)),
                                Move::Value,
                                vec![(n, int(0))],
                            ) {
                                let (mut lo, mut hi) = (0, x);
                                while hi - lo > 1 && !self.over() {
                                    let mid = lo + (hi - lo) / 2;
                                    if self.consider(
                                        g,
                                        g.set_value(n, i, int(mid)),
                                        Move::Value,
                                        vec![(n, int(mid))],
                                    ) {
                                        hi = mid;
                                    } else {
                                        lo = mid;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
    }

    fn shrink(&mut self, start: &Graph) -> Graph {
        let mut g = start.normalized();
        self.track(&g);
        let (start_ok, learned) = self.accept(&g, &[]);
        self.stats.start_ok = start_ok;
        for run_values in learned {
            if !g.walks_shape(&run_values) {
                self.stats.linked += g.graft(&run_values, self.body.budget()) as u64;
                self.stats.learned += 1;
            }
        }
        self.warm_up(&mut g);
        for _ in 0..PASS_CAP {
            let before = g.key();
            let learned_before = self.stats.learned;
            self.pass_delete(&mut g);
            g = g.normalized();
            self.pass_contract(&mut g);
            g = g.normalized();
            self.pass_merge(&mut g);
            g = g.normalized();
            self.pass_values(&mut g);
            g = g.normalized();
            self.stats.passes += 1;
            if self.over() {
                self.stats.capped = true;
                break;
            }
            if g.key() == before && self.stats.learned == learned_before && !self.warm_up(&mut g)
            {
                break;
            }
        }
        g
    }
}

#[derive(Default, Debug)]
struct GraphStats {
    nodes: usize,
    edges: usize,
    paths: u64,
    shapes: usize,
    enumerated: usize,
    wrong: usize,
    malformed: usize,
}

fn graph_stats(body: Body, graph: &Graph) -> GraphStats {
    let paths = graph.paths(PATH_CAP);
    let mut shapes: Vec<String> = paths.iter().map(|p| shape(p)).collect();
    shapes.sort();
    shapes.dedup();
    let mut stats = GraphStats {
        nodes: graph.bfs_order().len(),
        edges: graph.edge_count(),
        paths: graph.count_paths(),
        shapes: shapes.len(),
        enumerated: paths.len(),
        ..GraphStats::default()
    };
    for p in &paths {
        match predicate(body, p) {
            Some(true) => {}
            Some(false) => stats.wrong += 1,
            None => stats.malformed += 1,
        }
    }
    stats
}

impl GraphStats {
    fn json(&self) -> String {
        format!(
            "\"nodes\":{},\"edges\":{},\"paths\":{},\"shapes\":{},\"enumerated\":{},\"wrong\":{},\"malformed\":{}",
            self.nodes, self.edges, self.paths, self.shapes, self.enumerated, self.wrong, self.malformed
        )
    }
}

#[derive(Default, Debug)]
struct Cold {
    reproduced: usize,
    diverged: usize,
    clean: usize,
}

fn cold(
    body: Body,
    hidden: &RefCell<Rng>,
    seed: &mut dyn FnMut() -> u64,
    graph: &Graph,
    r: usize,
) -> Cold {
    let mut shrinker = Shrinker {
        body,
        hidden,
        seed,
        k: 1,
        judge: Judge::Lenient,
        warmup: 0,
        stats: ShrinkStats::default(),
    };
    let graph = Arc::new(graph.clone());
    let at_depth = Arc::new(graph.at_depth(body.budget()));
    let mut c = Cold::default();
    for _ in 0..r {
        let rep = shrinker.replay(&graph, &at_depth);
        c.reproduced += rep.outcome.interesting as usize;
        c.diverged += rep.outcome.divergence.is_some() as usize;
        c.clean += (rep.outcome.interesting && rep.clean) as usize;
    }
    c
}

fn env_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const JUDGES: [Judge; 3] = [Judge::Strict, Judge::Lenient, Judge::Learn];

fn run_trial(
    body: Body,
    confirm: usize,
    trial: u64,
    r: usize,
    ks: &[usize],
    out: &mut impl Write,
) -> bool {
    let hidden = RefCell::new(Rng::new(
        trial.wrapping_mul(0xC0FFEE) ^ (confirm as u64).wrapping_mul(0xBEEF) ^ 0x5EED5,
    ));
    let mut seed = trial.wrapping_mul(1_000_003) ^ (confirm as u64) << 40;
    let mut next_seed = || {
        seed += 1;
        seed
    };

    let mut t0 = None;
    let mut discovery_attempts = 0u64;
    for _ in 0..DISCOVERY_CAP {
        discovery_attempts += 1;
        let out = run(
            body,
            &hidden,
            ReplayKind::Fresh {
                budget: body.budget(),
            },
            next_seed(),
        );
        if out.interesting {
            t0 = Some(out.realized);
            break;
        }
    }
    let Some(t0) = t0 else {
        return false;
    };

    let mut poolall: Vec<Vec<ChoiceValue>> = vec![t0.clone()];
    let mut pool10: Vec<Vec<ChoiceValue>> = vec![t0.clone()];
    let mut confirm_fails = 0;
    for _ in 0..confirm {
        let out = run(
            body,
            &hidden,
            ReplayKind::Set {
                timelines: &pool10,
                extend: EXTEND,
            },
            next_seed(),
        );
        if out.interesting {
            confirm_fails += 1;
            if !poolall.contains(&out.realized) {
                poolall.push(out.realized.clone());
                if pool10.len() < 10 {
                    pool10.push(out.realized);
                }
            }
        }
    }

    let mut trie = Graph::new();
    for run_values in &poolall {
        trie.insert(run_values);
    }
    let mut single = Graph::new();
    single.insert(&t0);
    let starts: Vec<(&str, Graph)> = vec![
        ("t0", single),
        ("exact", trie.merge(Merge::Exact)),
        ("compat", trie.merge(Merge::Compatible)),
    ];

    let mut fields: Vec<String> = Vec::new();
    for (name, graph) in &starts {
        let stats = graph_stats(body, graph);
        let c = cold(body, &hidden, &mut next_seed, graph, r);
        fields.push(format!(
            "\"start-{name}\":{{{},\"cold\":[{},{},{}]}}",
            stats.json(),
            c.reproduced,
            c.diverged,
            c.clean
        ));
    }
    for (name, graph) in &starts {
        for judge in JUDGES {
            for &k in ks {
                let mut shrinker = Shrinker {
                    body,
                    hidden: &hidden,
                    seed: &mut next_seed,
                    k,
                    judge,
                    warmup: env_or("WARMUP", 3),
                    stats: ShrinkStats::default(),
                };
                let shrunk = shrinker.shrink(graph);
                let stats = shrinker.stats.clone();
                let gs = graph_stats(body, &shrunk);
                if std::env::var("DUMP").is_ok_and(|d| d == format!("{name}-{}", judge.name())) {
                    eprintln!(
                        "== {} confirm={confirm} trial={trial} {name}-{}",
                        body.name(),
                        judge.name()
                    );
                    for (i, node) in shrunk.nodes.iter().enumerate() {
                        let edges: Vec<String> = node
                            .edges
                            .iter()
                            .map(|(v, t)| format!("{}->{t}", value_key(v)))
                            .collect();
                        eprintln!(
                            "  {i}{} [{}]",
                            if node.terminal { "*" } else { "" },
                            edges.join(" ")
                        );
                    }
                    for p in shrunk.paths(PATH_CAP) {
                        if predicate(body, &p) != Some(true) {
                            eprintln!(
                                "  wrong: {}",
                                p.iter().map(value_key).collect::<Vec<_>>().join(" ")
                            );
                        }
                    }
                }
                let c = cold(body, &hidden, &mut next_seed, &shrunk, r);
                let accepts: Vec<String> = MOVES
                    .iter()
                    .map(|m| format!("\"{}\":{}", m.name(), stats.accepts[*m as usize]))
                    .collect();
                let rejects: Vec<String> = MOVES
                    .iter()
                    .map(|m| format!("\"{}\":{}", m.name(), stats.rejects[*m as usize]))
                    .collect();
                fields.push(format!(
                    "\"{name}-{}-{k}\":{{{},\"execs\":{},\"passes\":{},\"capped\":{},\"free\":{},\"learned\":{},\"warmed\":{},\"linked\":{},\"redundant\":{},\"unexercised\":{},\"start_ok\":{},\"max_nodes\":{},\"max_edges\":{},\"accepts\":{{{}}},\"rejects\":{{{}}},\"cold\":[{},{},{}]}}",
                    judge.name(),
                    gs.json(),
                    stats.execs,
                    stats.passes,
                    stats.capped,
                    stats.free,
                    stats.learned,
                    stats.warmed,
                    stats.linked,
                    stats.redundant,
                    stats.unexercised,
                    stats.start_ok,
                    stats.max_nodes,
                    stats.max_edges,
                    accepts.join(","),
                    rejects.join(","),
                    c.reproduced,
                    c.diverged,
                    c.clean
                ));
            }
        }
    }
    let (ideal_nodes, ideal_edges) = body.ideal();
    writeln!(
        out,
        "{{\"body\":\"{}\",\"k\":{},\"confirm\":{confirm},\"trial\":{trial},\"r\":{r},\"ideal\":[{ideal_nodes},{ideal_edges}],\"discovery_attempts\":{discovery_attempts},\"confirm_fails\":{confirm_fails},\"poolall\":{},{}}}",
        body.name(),
        body.k,
        poolall.len(),
        fields.join(",")
    )
    .unwrap();
    true
}

fn main() {
    let default_bodies =
        "block2,block3,block4,block6,block8,shift2,shift3,shift4,shift6,shift8,sum2,sum3,sum4";
    let bodies: Vec<Body> = std::env::var("BODIES")
        .unwrap_or_else(|_| default_bodies.to_string())
        .split(',')
        .map(|s| parse_body(s.trim()).unwrap_or_else(|| panic!("unknown body {s}")))
        .collect();
    let ks: Vec<usize> = std::env::var("KS")
        .unwrap_or_else(|_| "5,20".to_string())
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "results.jsonl".to_string());
    let mut out = std::fs::File::create(&out_path).unwrap();
    let trials_n = env_or("TRIALS", 10) as u64;
    let r = env_or("R", 50);
    for body in &bodies {
        for &confirm in &CONFIRMS {
            let mut found = 0;
            for trial in 0..trials_n {
                if run_trial(*body, confirm, trial, r, &ks, &mut out) {
                    found += 1;
                }
                out.flush().unwrap();
            }
            eprintln!(
                "{} confirm={confirm}: {found}/{trials_n} trials",
                body.name()
            );
        }
    }
}
