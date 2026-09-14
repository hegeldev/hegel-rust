//! Experiment 017: the counterexample as a graph.
//! Spec and results: notes/experiments/017-graph/notes.md
//!
//! For each body and confirmation-batch size, a trial discovers a failing
//! run, builds the timeline pool the engine would (cap 10, live-set
//! replays of the growing pool, failing runs captured) and, from the same
//! captured runs, an uncapped pool and two merged graphs (exact suffix
//! identity; structural identity, which recombines values across runs).
//! Each representation is then replayed cold against fresh hidden coins.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use hegel_c::__bench::{
    replay_case, BigInt, ChoiceValue, DataSource, Divergence, ExternalReplay, Failure,
    ReplayKind, ReplayOutcome, TestCaseResult, ToPrimitive,
};

const EXTEND: usize = 4;
const CONFIRMS: [usize; 2] = [20, 100];
const POOL_CAP: usize = 10;
const DISCOVERY_CAP: u64 = 100_000;
const PATH_CAP: usize = 200_000;

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

fn value_key(v: &ChoiceValue) -> String {
    match v {
        ChoiceValue::Integer(n) => format!("i{n}"),
        ChoiceValue::Boolean(b) => format!("b{b}"),
        other => format!("{:?}", shape(std::slice::from_ref(other))),
    }
}

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
    Structural,
    Compatible,
}

impl Merge {
    fn name(self) -> &'static str {
        match self {
            Merge::Exact => "exact",
            Merge::Structural => "struct",
            Merge::Compatible => "compat",
        }
    }
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
        self.nodes.iter().map(|n| n.edges.len()).sum()
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

    /// Merge nodes with identical futures — exactly, or structurally (by
    /// the kinds of their draws), where values recombine across runs.
    /// Returns the merged graph and the number of edges dropped because
    /// one value led to two different merged states.
    fn merged(&self, mode: Merge) -> (Graph, usize) {
        let order = self.post_order();
        let mut sig_of: Vec<usize> = vec![usize::MAX; self.nodes.len()];
        let mut intern: HashMap<String, usize> = HashMap::new();
        for &n in &order {
            let node = &self.nodes[n];
            let mut items: Vec<String> = node
                .edges
                .iter()
                .map(|(v, c)| match mode {
                    Merge::Exact => format!("{}={}", value_key(v), sig_of[*c]),
                    _ => format!("{}>{}", shape(std::slice::from_ref(v)), sig_of[*c]),
                })
                .collect();
            if mode == Merge::Structural {
                items.sort();
                items.dedup();
            }
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
        let mut conflicts = 0;
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
                match merged.nodes[m].edges.iter().find(|(e, _)| e == v) {
                    Some(&(_, t)) if t == target => {}
                    Some(_) => conflicts += 1,
                    None => merged.nodes[m].edges.push((v.clone(), target)),
                }
            }
            map[n] = m;
        }
        merged.root = map[self.root];
        (merged, conflicts)
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
            let kind = shape(std::slice::from_ref(&v));
            let targets: Vec<usize> = self.nodes[a]
                .edges
                .iter()
                .filter(|(e, _)| shape(std::slice::from_ref(e)) == kind)
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

    fn reaches(&self, parent: &mut [usize], from: usize, to: usize) -> bool {
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

    fn related(&self, parent: &mut [usize], a: usize, b: usize) -> bool {
        self.reaches(parent, a, b) || self.reaches(parent, b, a)
    }

    /// Fold `b` into `a`, and recursively the targets of their shared
    /// values; a fold that would close a cycle is left as a second edge
    /// with the same value instead.
    fn fold(&mut self, parent: &mut [usize], a: usize, b: usize) {
        let (a, b) = (find(parent, a), find(parent, b));
        if a == b {
            return;
        }
        parent[b] = a;
        let edges_b = std::mem::take(&mut self.nodes[b].edges);
        for (v, c) in edges_b {
            match self.nodes[a].edges.iter().find(|(e, _)| *e == v).map(|&(_, t)| t) {
                Some(t) if find(parent, t) == find(parent, c) => {}
                Some(t) if self.related(parent, t, c) => self.nodes[a].edges.push((v, c)),
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

    /// Greedy state merging in the manner of RPNI: states are visited
    /// breadth-first and each is folded into the earliest accepted state
    /// it is compatible with — same terminal standing, and for every
    /// kind of draw both have observed, compatible futures. Disjoint
    /// observations never conflict, so this generalizes furthest.
    fn merged_compatible(&self) -> (Graph, usize) {
        let mut g = self.clone();
        let mut parent: Vec<usize> = (0..g.nodes.len()).collect();
        let mut accepted: Vec<usize> = Vec::new();
        let mut merges = 0;
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
                if !g.related(&mut parent, a, n) && g.compatible(&mut parent, a, n, &mut assumed) {
                    g.fold(&mut parent, a, n);
                    merges += 1;
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
        (merged, merges)
    }

    fn merge(&self, mode: Merge) -> (Graph, usize) {
        match mode {
            Merge::Compatible => self.merged_compatible(),
            other => self.merged(other),
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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rescue {
    Random,
    Skip,
    Positional,
    Rejoin,
}

impl Rescue {
    fn name(self) -> &'static str {
        match self {
            Rescue::Random => "random",
            Rescue::Skip => "skip",
            Rescue::Positional => "positional",
            Rescue::Rejoin => "rejoin",
        }
    }
}

const RESCUES: [Rescue; 4] = [
    Rescue::Random,
    Rescue::Skip,
    Rescue::Positional,
    Rescue::Rejoin,
];
const MERGES: [Merge; 3] = [Merge::Exact, Merge::Structural, Merge::Compatible];

struct Walk {
    graph: Arc<Graph>,
    at_depth: Arc<Vec<Vec<usize>>>,
    node: Option<usize>,
    rescue: Rescue,
    divergence: Option<Divergence>,
    longest: usize,
}

impl Walk {
    fn new(graph: Arc<Graph>, at_depth: Arc<Vec<Vec<usize>>>, rescue: Rescue) -> Self {
        let longest = graph.depth();
        Walk {
            node: Some(graph.root),
            graph,
            at_depth,
            rescue,
            divergence: None,
            longest,
        }
    }

    fn donor(&self, position: usize, fits: &dyn Fn(&ChoiceValue) -> bool) -> Option<(ChoiceValue, usize)> {
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
                return Some(v.clone());
            }
            if self.divergence.is_none() {
                self.divergence = Some(Divergence {
                    stream: Vec::new(),
                    position,
                });
            }
            self.node = match self.rescue {
                Rescue::Skip => node.edges.first().map(|&(_, t)| t),
                _ => None,
            };
            if self.rescue != Rescue::Rejoin {
                return None;
            }
        }
        match self.rescue {
            Rescue::Positional => self.donor(position, fits).map(|(v, _)| v),
            Rescue::Rejoin => {
                let (v, t) = self.donor(position, fits)?;
                self.node = Some(t);
                Some(v)
            }
            _ => None,
        }
    }

    fn divergence(&self) -> Option<Divergence> {
        self.divergence.clone()
    }

    fn longest(&self) -> usize {
        self.longest
    }
}

#[derive(Default, Clone, Debug)]
struct Cell {
    reproduced: usize,
    diverged: usize,
}

impl Cell {
    fn add(&mut self, out: &ReplayOutcome) {
        self.reproduced += out.interesting as usize;
        self.diverged += out.divergence.is_some() as usize;
    }
}

#[derive(Default, Debug)]
struct Trial {
    discovery_attempts: u64,
    confirm_fails: usize,
    pool10: usize,
    pool10_shapes: usize,
    poolall: usize,
    poolall_shapes: usize,
    trie_nodes: usize,
    trie_edges: usize,
    graphs: Vec<GraphStats>,
    cells: Vec<(String, Cell)>,
}

#[derive(Default, Debug)]
struct GraphStats {
    name: String,
    nodes: usize,
    edges: usize,
    merges: usize,
    paths: u64,
    shapes: usize,
    enumerated: usize,
    wrong: usize,
    malformed: usize,
}

fn graph_stats(body: Body, name: &str, graph: &Graph, merges: usize) -> GraphStats {
    let paths = graph.paths(PATH_CAP);
    let mut shapes: Vec<String> = paths.iter().map(|p| shape(p)).collect();
    shapes.sort();
    shapes.dedup();
    let mut stats = GraphStats {
        name: name.to_string(),
        nodes: graph.nodes.len(),
        edges: graph.edge_count(),
        merges,
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

fn env_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn run_trial(body: Body, confirm: usize, trial: u64, r: usize) -> Option<Trial> {
    let hidden = RefCell::new(Rng::new(
        trial.wrapping_mul(0xC0FFEE) ^ (confirm as u64).wrapping_mul(0xBEEF) ^ 0xD15EA5E,
    ));
    let mut seed = trial.wrapping_mul(1_000_003) ^ (confirm as u64) << 40;
    let mut next_seed = || {
        seed += 1;
        seed
    };
    let mut t = Trial::default();

    let mut t0 = None;
    for _ in 0..DISCOVERY_CAP {
        t.discovery_attempts += 1;
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
    let t0 = t0?;

    let mut pool10: Vec<Vec<ChoiceValue>> = vec![t0.clone()];
    let mut poolall: Vec<Vec<ChoiceValue>> = vec![t0.clone()];
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
            t.confirm_fails += 1;
            if !poolall.contains(&out.realized) {
                poolall.push(out.realized.clone());
                if pool10.len() < POOL_CAP {
                    pool10.push(out.realized);
                }
            }
        }
    }
    let distinct_shapes = |pool: &[Vec<ChoiceValue>]| {
        let mut shapes: Vec<String> = pool.iter().map(|p| shape(p)).collect();
        shapes.sort();
        shapes.dedup();
        shapes.len()
    };
    t.pool10 = pool10.len();
    t.pool10_shapes = distinct_shapes(&pool10);
    t.poolall = poolall.len();
    t.poolall_shapes = distinct_shapes(&poolall);

    let mut trie = Graph::new();
    for run_values in &poolall {
        trie.insert(run_values);
    }
    t.trie_nodes = trie.nodes.len();
    t.trie_edges = trie.edge_count();
    let mut graphs: Vec<(Merge, Arc<Graph>, Arc<Vec<Vec<usize>>>)> = Vec::new();
    for mode in MERGES {
        let (graph, merges) = trie.merge(mode);
        t.graphs.push(graph_stats(body, mode.name(), &graph, merges));
        let at_depth = Arc::new(graph.at_depth(body.budget()));
        graphs.push((mode, Arc::new(graph), at_depth));
    }

    let mut cells: Vec<(String, Cell)> = ["fresh", "t0", "pool10", "poolall"]
        .iter()
        .map(|s| (s.to_string(), Cell::default()))
        .collect();
    for mode in MERGES {
        for rescue in RESCUES {
            cells.push((format!("{}-{}", mode.name(), rescue.name()), Cell::default()));
        }
    }
    for _ in 0..r {
        for (name, cell) in cells.iter_mut() {
            let seed = next_seed();
            let out = match name.as_str() {
                "fresh" => run(
                    body,
                    &hidden,
                    ReplayKind::Fresh {
                        budget: body.budget(),
                    },
                    seed,
                ),
                "t0" => run(
                    body,
                    &hidden,
                    ReplayKind::Sequence {
                        choices: &t0,
                        extend: EXTEND,
                    },
                    seed,
                ),
                "pool10" => run(
                    body,
                    &hidden,
                    ReplayKind::Set {
                        timelines: &pool10,
                        extend: EXTEND,
                    },
                    seed,
                ),
                "poolall" => run(
                    body,
                    &hidden,
                    ReplayKind::Set {
                        timelines: &poolall,
                        extend: EXTEND,
                    },
                    seed,
                ),
                other => {
                    let (mode_name, rescue_name) = other.split_once('-').unwrap();
                    let (_, graph, at_depth) = graphs
                        .iter()
                        .find(|(m, _, _)| m.name() == mode_name)
                        .unwrap();
                    let rescue = RESCUES.iter().find(|r| r.name() == rescue_name).unwrap();
                    let walk = Walk::new(Arc::clone(graph), Arc::clone(at_depth), *rescue);
                    let budget = walk.longest + EXTEND;
                    run(
                        body,
                        &hidden,
                        ReplayKind::External {
                            resolver: Box::new(walk),
                            budget,
                        },
                        seed,
                    )
                }
            };
            cell.add(&out);
        }
    }
    t.cells = cells;
    Some(t)
}

fn main() {
    let bodies: Vec<Body> = [2usize, 3, 4, 6, 8]
        .iter()
        .flat_map(|&k| [Kind::Block, Kind::Shift].map(|kind| Body { kind, k }))
        .chain([2usize, 3, 4].iter().map(|&k| Body { kind: Kind::Sum, k }))
        .collect();
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "results.jsonl".to_string());
    let mut out = std::fs::File::create(&out_path).unwrap();
    let trials_n = env_or("TRIALS", 20) as u64;
    let r = env_or("R", 50);
    for body in &bodies {
        for &confirm in &CONFIRMS {
            let mut found = 0;
            for trial in 0..trials_n {
                let Some(t) = run_trial(*body, confirm, trial, r) else {
                    continue;
                };
                found += 1;
                let cells: Vec<String> = t
                    .cells
                    .iter()
                    .map(|(n, c)| format!("\"{n}\":[{},{}]", c.reproduced, c.diverged))
                    .collect();
                let graphs: Vec<String> = t
                    .graphs
                    .iter()
                    .map(|g| {
                        format!(
                            "\"{}\":{{\"nodes\":{},\"edges\":{},\"merges\":{},\"paths\":{},\"shapes\":{},\"enumerated\":{},\"wrong\":{},\"malformed\":{}}}",
                            g.name, g.nodes, g.edges, g.merges, g.paths, g.shapes, g.enumerated, g.wrong, g.malformed
                        )
                    })
                    .collect();
                writeln!(
                    out,
                    "{{\"body\":\"{}\",\"k\":{},\"confirm\":{confirm},\"trial\":{trial},\"r\":{r},\"discovery_attempts\":{},\"confirm_fails\":{},\"pool10\":{},\"pool10_shapes\":{},\"poolall\":{},\"poolall_shapes\":{},\"trie\":[{},{}],\"graphs\":{{{}}},\"cells\":{{{}}}}}",
                    body.name(),
                    body.k,
                    t.discovery_attempts,
                    t.confirm_fails,
                    t.pool10,
                    t.pool10_shapes,
                    t.poolall,
                    t.poolall_shapes,
                    t.trie_nodes,
                    t.trie_edges,
                    graphs.join(","),
                    cells.join(",")
                )
                .unwrap();
            }
            eprintln!("{} confirm={confirm}: {found}/{trials_n} trials", body.name());
        }
    }
}
