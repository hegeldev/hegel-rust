//! Experiment 019: state identity from the span structure.
//! Spec and results: notes/experiments/019-graph-identity/notes.md
//!
//! Experiment 018's shrinker identified states by depth (draws so far),
//! which aliases the unequal arms of `shift`. Here every draw carries its
//! structural address — the spans open at the draw, each as (label, number
//! of earlier same-label siblings) — passed by the engine through the
//! `ExternalReplay` hook, and a state between two draws is identified by
//! the prefix of the next draw's address through its first frame that was
//! not open at the previous draw. Node identity replaces depth in the
//! walk's rejoin, in the graft and in the merge move; the walk checks the
//! identity it reaches against the node it is on, so a wrong join is seen;
//! and an edge may lead to several nodes, the one taken decided by the
//! identity the next draw reports (a tie), which is how a structure decided
//! by a hidden coin after a draw is represented. Bodies open spans as real
//! generators do; two new bodies have a structure that depends on a drawn
//! value (`list`) and on a hidden coin (`loop`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use hegel_c::__bench::{
    replay_case, BigInt, ChoiceValue, DataSource, Divergence, ExternalReplay, Failure, ReplayKind,
    ReplayOutcome, TestCaseResult, ToPrimitive,
};
use hegel_c::hegel_label_t;

const EXTEND: usize = 4;
const CONFIRMS: [usize; 2] = [20, 100];
const DISCOVERY_CAP: u64 = 100_000;
const PATH_CAP: usize = 200_000;
const EXEC_CAP: u64 = 100_000;
const PASS_CAP: usize = 20;

const LABEL_INT: u64 = hegel_label_t::HEGEL_LABEL_INTEGER as u64;
const LABEL_BOOL: u64 = hegel_label_t::HEGEL_LABEL_BOOLEAN as u64;
const LABEL_PIECE: u64 = 1001;
const LABEL_ARM: u64 = 1002;

type Frame = (u64, usize);
type Addr = Vec<Frame>;
type Ident = Vec<Frame>;

fn lcp(a: &[Frame], b: &[Frame]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// The identity of the state between a draw at `prev` and one at `next`:
/// the prefix of `next` through its first frame that was not open at
/// `prev`. The state after the last draw is `END`, the empty identity.
fn ident_after(prev: &[Frame], next: &[Frame]) -> Ident {
    let l = lcp(prev, next);
    assert!(
        l < next.len(),
        "a draw's address is never a prefix of the next draw's"
    );
    next[..=l].to_vec()
}

fn fmt_frames(frames: &[Frame]) -> String {
    let names: Vec<String> = frames
        .iter()
        .map(|(l, o)| {
            let name = match *l {
                LABEL_INT => "i".to_string(),
                LABEL_BOOL => "b".to_string(),
                LABEL_PIECE => "P".to_string(),
                LABEL_ARM => "A".to_string(),
                other => other.to_string(),
            };
            format!("{name}{o}")
        })
        .collect();
    names.join("/")
}

#[derive(Clone, Debug, PartialEq)]
struct Step {
    addr: Addr,
    value: ChoiceValue,
}

#[derive(Clone, Debug, PartialEq)]
struct Run {
    steps: Vec<Step>,
}

impl Run {
    /// The identities of the states along the run: before the first draw,
    /// after each draw, the last being `END`.
    fn idents(&self) -> Vec<Ident> {
        let mut out = Vec::with_capacity(self.steps.len() + 1);
        let mut prev: &[Frame] = &[];
        for s in &self.steps {
            out.push(ident_after(prev, &s.addr));
            prev = &s.addr;
        }
        out.push(Vec::new());
        out
    }

    fn describe(&self) -> String {
        self.steps
            .iter()
            .map(|s| format!("{}={}", fmt_frames(&s.addr), value_key(&s.value)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

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
    List,
    Loop,
}

#[derive(Clone, Copy, Debug)]
struct Body {
    kind: Kind,
    k: usize,
}

const LOOP_CONTINUE: f64 = 0.75;

impl Body {
    fn name(&self) -> String {
        let kind = match self.kind {
            Kind::Block => "block",
            Kind::Shift => "shift",
            Kind::Sum => "sum",
            Kind::List => "list",
            Kind::Loop => "loop",
        };
        format!("{kind}{}", self.k)
    }

    fn budget(&self) -> usize {
        2 + 2 * self.k + EXTEND
    }

    /// The smallest graph that reproduces the failure on every hidden coin.
    fn ideal(&self) -> (usize, usize) {
        match self.kind {
            Kind::Shift => (2 + 2 * self.k, 1 + 3 * self.k),
            Kind::List => (4, 4),
            Kind::Loop => (3 + self.k, 1 + 4 * self.k),
            _ => (2 + self.k, 1 + 2 * self.k),
        }
    }

    /// The number of structures (hidden-coin outcomes) the failure has.
    fn shapes_total(&self) -> u64 {
        match self.kind {
            Kind::List => 2,
            Kind::Loop => (1u64 << (self.k + 1)) - 1,
            _ => 1u64 << self.k,
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
        "list" => Kind::List,
        "loop" => Kind::Loop,
        _ => return None,
    };
    Some(Body { kind, k })
}

struct Piece {
    hot: bool,
    sum: i64,
}

fn body_draws(body: Body, hidden: &mut Rng, ds: &dyn DataSource) -> Result<bool, ()> {
    let int = |lo: i64, hi: i64| -> Result<i64, ()> {
        ds.generate_integer(&BigInt::from(lo), &BigInt::from(hi))
            .map(|v| v.to_i64().unwrap())
            .map_err(|_| ())
    };
    let coin = || ds.generate_boolean(0.5, None).map_err(|_| ());
    let span = |label: u64| ds.start_span(label).map_err(|_| ());
    let close = || ds.stop_span(false).map_err(|_| ());
    let piece = |hidden: &mut Rng, acc: &mut Piece| -> Result<(), ()> {
        span(LABEL_PIECE)?;
        if hidden.f64() < 0.5 {
            let b = coin()?;
            acc.hot &= b;
            acc.sum += if b { 100 } else { 0 };
        } else {
            let x = if body.kind == Kind::Shift {
                span(LABEL_ARM)?;
                let x = int(0, 100)?;
                coin()?;
                close()?;
                x
            } else {
                int(0, 100)?
            };
            acc.hot &= x >= 60;
            acc.sum += x;
        }
        close()
    };
    let a = coin()?;
    let mut acc = Piece { hot: true, sum: 0 };
    match body.kind {
        Kind::Block | Kind::Shift | Kind::Sum => {
            for _ in 0..body.k {
                piece(hidden, &mut acc)?;
            }
        }
        Kind::List => {
            let n = int(0, body.k as i64)?;
            for _ in 0..n {
                piece(hidden, &mut acc)?;
            }
            return Ok(a && n >= 1 && acc.hot);
        }
        Kind::Loop => {
            let mut m = 0;
            while m < body.k && hidden.f64() < LOOP_CONTINUE {
                piece(hidden, &mut acc)?;
                m += 1;
            }
            let z = coin()?;
            return Ok(a && acc.hot && z);
        }
    }
    Ok(match body.kind {
        Kind::Sum => a && acc.sum >= 60 * body.k as i64,
        _ => a && acc.hot,
    })
}

struct Cursor<'a> {
    steps: &'a [Step],
    i: usize,
}

impl<'a> Cursor<'a> {
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

    fn bool(&mut self, addr: &[Frame]) -> Option<bool> {
        match self.take(addr)? {
            ChoiceValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    fn int(&mut self, addr: &[Frame]) -> Option<i64> {
        match self.take(addr)? {
            ChoiceValue::Integer(x) => x.to_i64(),
            _ => None,
        }
    }

    fn done(&self) -> Option<()> {
        (self.i == self.steps.len()).then_some(())
    }
}

/// The body's verdict on a whole path, read off its structure and values:
/// `Some(fail)` for a path some run of the body produces, `None` otherwise.
fn predicate(body: Body, steps: &[Step]) -> Option<bool> {
    let mut c = Cursor { steps, i: 0 };
    let a = c.bool(&[(LABEL_BOOL, 0)])?;
    let mut acc = Piece { hot: true, sum: 0 };
    let piece = |c: &mut Cursor, j: usize, acc: &mut Piece| -> Option<()> {
        let p = (LABEL_PIECE, j);
        let next = c.peek()?;
        if next.addr.first() != Some(&p) {
            return None;
        }
        match next.addr.get(1)?.0 {
            LABEL_BOOL => {
                let b = c.bool(&[p, (LABEL_BOOL, 0)])?;
                acc.hot &= b;
                acc.sum += if b { 100 } else { 0 };
            }
            LABEL_INT if body.kind != Kind::Shift => {
                let x = c.int(&[p, (LABEL_INT, 0)])?;
                acc.hot &= x >= 60;
                acc.sum += x;
            }
            LABEL_ARM if body.kind == Kind::Shift => {
                let x = c.int(&[p, (LABEL_ARM, 0), (LABEL_INT, 0)])?;
                c.bool(&[p, (LABEL_ARM, 0), (LABEL_BOOL, 0)])?;
                acc.hot &= x >= 60;
                acc.sum += x;
            }
            _ => return None,
        }
        Some(())
    };
    match body.kind {
        Kind::Block | Kind::Shift | Kind::Sum => {
            for j in 0..body.k {
                piece(&mut c, j, &mut acc)?;
            }
            c.done()?;
            Some(match body.kind {
                Kind::Sum => a && acc.sum >= 60 * body.k as i64,
                _ => a && acc.hot,
            })
        }
        Kind::List => {
            let n = c.int(&[(LABEL_INT, 0)])?;
            if n < 0 || n > body.k as i64 {
                return None;
            }
            for j in 0..n as usize {
                piece(&mut c, j, &mut acc)?;
            }
            c.done()?;
            Some(a && n >= 1 && acc.hot)
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
                piece(&mut c, j, &mut acc)?;
                j += 1;
            }
            let z = c.bool(&[(LABEL_BOOL, 1)])?;
            c.done()?;
            Some(a && acc.hot && z)
        }
    }
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

#[derive(Clone, Debug, PartialEq)]
struct Edge {
    addr: Addr,
    value: ChoiceValue,
    target: usize,
}

#[derive(Clone, Debug)]
struct Node {
    id: Ident,
    edges: Vec<Edge>,
}

impl Node {
    fn new(id: Ident) -> Node {
        Node {
            id,
            edges: Vec::new(),
        }
    }

    fn terminal(&self) -> bool {
        self.id.is_empty()
    }
}

#[derive(Clone, Debug)]
struct Graph {
    nodes: Vec<Node>,
    root: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Walked {
    Whole,
    Foreign,
    Gap,
}

impl Graph {
    fn from_runs(runs: &[Run]) -> Graph {
        let mut g = Graph {
            nodes: vec![Node::new(runs[0].idents()[0].clone())],
            root: 0,
        };
        for run in runs {
            g.insert(run);
        }
        g
    }

    fn end_node(&mut self) -> usize {
        match self.nodes.iter().position(Node::terminal) {
            Some(n) => n,
            None => {
                self.nodes.push(Node::new(Vec::new()));
                self.nodes.len() - 1
            }
        }
    }

    fn fresh(&mut self, id: &Ident) -> usize {
        if id.is_empty() {
            return self.end_node();
        }
        self.nodes.push(Node::new(id.clone()));
        self.nodes.len() - 1
    }

    fn insert(&mut self, run: &Run) {
        let ids = run.idents();
        let mut n = self.root;
        assert_eq!(self.nodes[n].id, ids[0], "runs of one body start alike");
        for (i, s) in run.steps.iter().enumerate() {
            let want = &ids[i + 1];
            let next = self.nodes[n]
                .edges
                .iter()
                .find(|e| {
                    e.addr == s.addr && e.value == s.value && self.nodes[e.target].id == *want
                })
                .map(|e| e.target);
            n = match next {
                Some(t) => t,
                None => {
                    let t = self.fresh(want);
                    self.nodes[n].edges.push(Edge {
                        addr: s.addr.clone(),
                        value: s.value.clone(),
                        target: t,
                    });
                    t
                }
            };
        }
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
            for e in &g.nodes[n].edges {
                visit(g, e.target, seen, order);
            }
            order.push(n);
        }
        visit(self, self.root, &mut seen, &mut order);
        order
    }

    /// Merge nodes with identical identity and identical futures, value
    /// for value (017's exact merge).
    fn merged_exact(&self) -> Graph {
        let order = self.post_order();
        let mut sig_of: Vec<usize> = vec![usize::MAX; self.nodes.len()];
        let mut intern: HashMap<String, usize> = HashMap::new();
        for &n in &order {
            let node = &self.nodes[n];
            let items: Vec<String> = node
                .edges
                .iter()
                .map(|e| format!("{:?}={}={}", e.addr, value_key(&e.value), sig_of[e.target]))
                .collect();
            let key = format!("{:?}|{}", node.id, items.join(","));
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
                merged.nodes.push(Node::new(self.nodes[n].id.clone()));
                merged.nodes.len() - 1
            });
            for e in &self.nodes[n].edges {
                let e2 = Edge {
                    addr: e.addr.clone(),
                    value: e.value.clone(),
                    target: map[e.target],
                };
                if !merged.nodes[m].edges.contains(&e2) {
                    merged.nodes[m].edges.push(e2);
                }
            }
            map[n] = m;
        }
        merged.root = map[self.root];
        merged
    }

    /// Merge every pair of nodes with the same identity: the structural
    /// automaton of the runs.
    fn merged_ident(&self) -> Graph {
        let order = self.bfs_order();
        let mut canon: HashMap<Ident, usize> = HashMap::new();
        let mut merged = Graph {
            nodes: Vec::new(),
            root: 0,
        };
        let mut map = vec![usize::MAX; self.nodes.len()];
        for &n in &order {
            let id = &self.nodes[n].id;
            let next = merged.nodes.len();
            map[n] = *canon.entry(id.clone()).or_insert_with(|| {
                merged.nodes.push(Node::new(id.clone()));
                next
            });
        }
        for &n in &order {
            for e in &self.nodes[n].edges {
                let e2 = Edge {
                    addr: e.addr.clone(),
                    value: e.value.clone(),
                    target: map[e.target],
                };
                if !merged.nodes[map[n]].edges.contains(&e2) {
                    merged.nodes[map[n]].edges.push(e2);
                }
            }
        }
        merged.root = map[self.root];
        assert!(merged.is_acyclic(), "identity merge closed a cycle");
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
            for e in &self.nodes[n].edges {
                if !seen[e.target] {
                    seen[e.target] = true;
                    order.push(e.target);
                }
            }
        }
        order
    }

    /// Reachable nodes by identity, in breadth-first order.
    fn by_ident(&self) -> HashMap<Ident, Vec<usize>> {
        let mut index: HashMap<Ident, Vec<usize>> = HashMap::new();
        for n in self.bfs_order() {
            index.entry(self.nodes[n].id.clone()).or_default().push(n);
        }
        index
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
            for e in &g.nodes[n].edges {
                if !visit(g, e.target, state) {
                    return false;
                }
            }
            state[n] = 2;
            true
        }
        visit(self, self.root, &mut state)
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
                .map(|e| 1 + d(g, e.target, memo))
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
            let mut total: u64 = if node.terminal() { 1 } else { 0 };
            for e in &node.edges {
                total = total.saturating_add(c(g, e.target, memo));
            }
            memo[n] = Some(total);
            total
        }
        c(self, self.root, &mut memo)
    }

    fn paths(&self, cap: usize) -> Vec<Vec<Step>> {
        let mut out = Vec::new();
        let mut stack: Vec<Step> = Vec::new();
        fn walk(g: &Graph, n: usize, stack: &mut Vec<Step>, out: &mut Vec<Vec<Step>>, cap: usize) {
            if out.len() >= cap {
                return;
            }
            let node = &g.nodes[n];
            if node.terminal() {
                out.push(stack.clone());
            }
            for e in &node.edges {
                stack.push(Step {
                    addr: e.addr.clone(),
                    value: e.value.clone(),
                });
                walk(g, e.target, stack, out, cap);
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
            for e in &self.nodes[n].edges {
                values.push(value_rank(&e.value));
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
                id: self.nodes[n].id.clone(),
                edges: self.nodes[n]
                    .edges
                    .iter()
                    .map(|e| Edge {
                        addr: e.addr.clone(),
                        value: e.value.clone(),
                        target: map[e.target],
                    })
                    .collect(),
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
        let mut stack = vec![from];
        let mut seen = vec![false; self.nodes.len()];
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if seen[n] {
                continue;
            }
            seen[n] = true;
            for e in &self.nodes[n].edges {
                stack.push(e.target);
            }
        }
        false
    }

    /// The edges into `n`, as (source node, value): what a walk serves when
    /// it takes one of them.
    fn edges_into(&self, n: usize) -> Vec<(usize, ChoiceValue)> {
        let mut into = Vec::new();
        for p in self.bfs_order() {
            for e in &self.nodes[p].edges {
                if e.target == n {
                    into.push((p, e.value.clone()));
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

    /// Redirect every edge into `n` to its successor `t` (dropping a
    /// redirected edge `p` already has), deleting `n` and its edges.
    fn contract(&self, n: usize, t: usize) -> Graph {
        let mut g = self.clone();
        for p in 0..g.nodes.len() {
            let mut kept: Vec<Edge> = Vec::new();
            for mut e in std::mem::take(&mut g.nodes[p].edges) {
                if e.target == n {
                    e.target = t;
                }
                if !kept.contains(&e) {
                    kept.push(e);
                }
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
                if e.target == b {
                    e.target = a;
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
        g.nodes[n].edges[i].value = v;
        g
    }

    /// How the first-fit walk (ties settled by identity) relates to a run:
    /// it walks the whole run; it would have served a different value at
    /// some draw, so no replay of this graph produces the run (`Foreign`);
    /// or it has no edge for a draw, or no target of the reported identity,
    /// or does not end on `END` — a structural gap the run fills.
    fn walk_verdict(&self, run: &Run) -> Walked {
        let ids = run.idents();
        let mut n = self.root;
        if self.nodes[n].id != ids[0] {
            return Walked::Gap;
        }
        for (i, s) in run.steps.iter().enumerate() {
            let node = &self.nodes[n];
            let Some(e) = node
                .edges
                .iter()
                .find(|e| e.addr == s.addr && kind_of(&e.value) == kind_of(&s.value))
            else {
                return Walked::Gap;
            };
            if e.value != s.value {
                return Walked::Foreign;
            }
            let next = node
                .edges
                .iter()
                .filter(|f| f.addr == e.addr && f.value == e.value)
                .map(|f| f.target)
                .find(|&t| self.nodes[t].id == ids[i + 1]);
            match next {
                Some(t) => n = t,
                None => return Walked::Gap,
            }
        }
        if self.nodes[n].terminal() {
            Walked::Whole
        } else {
            Walked::Gap
        }
    }

    /// Whether `steps` (with the identities after each) is a path from `n`
    /// ending on `END`.
    fn has_path(&self, mut n: usize, steps: &[Step], ids: &[Ident]) -> bool {
        for (s, want) in steps.iter().zip(ids) {
            let next = self.nodes[n]
                .edges
                .iter()
                .find(|e| {
                    e.addr == s.addr && e.value == s.value && self.nodes[e.target].id == *want
                })
                .map(|e| e.target);
            match next {
                Some(t) => n = t,
                None => return false,
            }
        }
        self.nodes[n].terminal()
    }

    /// Learn a failing run the graph does not contain: follow it while it
    /// fits, identity for identity; where it departs, link to a node of the
    /// identity the run reaches from which the rest of the run is already a
    /// path (the rejoin the rescue made, made permanent), and only where no
    /// such node exists append the rest as new nodes. Returns the number of
    /// edges linked.
    fn graft(&mut self, run: &Run) -> usize {
        let ids = run.idents();
        let index = self.by_ident();
        let mut n = self.root;
        assert_eq!(self.nodes[n].id, ids[0], "runs of one body start alike");
        for (i, s) in run.steps.iter().enumerate() {
            let want = &ids[i + 1];
            let next = self.nodes[n]
                .edges
                .iter()
                .find(|e| {
                    e.addr == s.addr && e.value == s.value && self.nodes[e.target].id == *want
                })
                .map(|e| e.target);
            if let Some(t) = next {
                n = t;
                continue;
            }
            let donor = index.get(want).and_then(|cands| {
                cands.iter().copied().find(|&m| {
                    self.has_path(m, &run.steps[i + 1..], &ids[i + 2..]) && !self.reaches(m, n)
                })
            });
            let t = match donor {
                Some(m) => m,
                None => self.fresh(want),
            };
            self.nodes[n].edges.push(Edge {
                addr: s.addr.clone(),
                value: s.value.clone(),
                target: t,
            });
            if donor.is_some() {
                return 1;
            }
            n = t;
        }
        0
    }

    fn dump(&self) {
        for (i, node) in self.nodes.iter().enumerate() {
            let edges: Vec<String> = node
                .edges
                .iter()
                .map(|e| {
                    format!(
                        "{}={}->{}",
                        fmt_frames(&e.addr),
                        value_key(&e.value),
                        e.target
                    )
                })
                .collect();
            eprintln!(
                "  {i} <{}>{} [{}]",
                fmt_frames(&node.id),
                if node.terminal() { "*" } else { "" },
                edges.join(" ")
            );
        }
    }
}

/// What a walk reports back: the node it is on (of a tie, a terminal one if
/// any), the edges it settled on as (node, value, target) — an edge counts
/// once the next draw's identity picked its target from the tie, or the run
/// ended on a terminal target — the edge last served and still unsettled,
/// the address of every draw, and how often it was misjoined or rescued.
#[derive(Default)]
struct Trace {
    node: Option<usize>,
    served: Vec<(usize, ChoiceValue, usize)>,
    open: Option<(usize, ChoiceValue, Vec<usize>)>,
    addrs: Vec<Addr>,
    misjoins: u32,
    rescues: u32,
}

type Watch = Vec<(usize, ChoiceValue, usize)>;

struct Walk {
    graph: Arc<Graph>,
    index: Arc<HashMap<Ident, Vec<usize>>>,
    prev: Addr,
    pending: Vec<usize>,
    trace: Arc<Mutex<Trace>>,
    divergence: Option<Divergence>,
    longest: usize,
}

impl Walk {
    fn new(
        graph: Arc<Graph>,
        index: Arc<HashMap<Ident, Vec<usize>>>,
        trace: Arc<Mutex<Trace>>,
    ) -> Self {
        let longest = graph.depth();
        trace.lock().unwrap().node = Some(graph.root);
        Walk {
            pending: vec![graph.root],
            graph,
            index,
            prev: Vec::new(),
            trace,
            divergence: None,
            longest,
        }
    }

    fn diverge(&mut self, position: usize) {
        if self.divergence.is_none() {
            self.divergence = Some(Divergence {
                stream: Vec::new(),
                position,
            });
        }
    }

    fn serve(&mut self, n: usize, e: Edge, trace: &mut Trace) -> ChoiceValue {
        self.pending = self.graph.nodes[n]
            .edges
            .iter()
            .filter(|f| f.addr == e.addr && f.value == e.value)
            .map(|f| f.target)
            .collect();
        trace.node = self
            .pending
            .iter()
            .copied()
            .find(|&t| self.graph.nodes[t].terminal())
            .or(self.pending.first().copied());
        trace.open = Some((n, e.value.clone(), self.pending.clone()));
        e.value
    }

    fn fitting(
        &self,
        n: usize,
        addr: &[Frame],
        fits: &dyn Fn(&ChoiceValue) -> bool,
    ) -> Option<Edge> {
        self.graph.nodes[n]
            .edges
            .iter()
            .find(|e| e.addr == addr && fits(&e.value))
            .cloned()
    }
}

/// The graph walk: the node reached is the one, of those the served edge
/// leads to, whose identity the draw's address reports (a misjoin if none);
/// the draw is served first-fit from that node's edges at this address (a
/// misfit if none); on either, or when lost, the walk is rescued by the
/// first node of the reported identity with a fitting edge.
impl ExternalReplay for Walk {
    fn resolve(
        &mut self,
        stream: &[usize],
        position: usize,
        frames: &[Frame],
        fits: &dyn Fn(&ChoiceValue) -> bool,
    ) -> Option<ChoiceValue> {
        if !stream.is_empty() {
            return None;
        }
        let ident = ident_after(&self.prev, frames);
        self.prev = frames.to_vec();
        let trace = Arc::clone(&self.trace);
        let mut trace = trace.lock().unwrap();
        trace.addrs.push(frames.to_vec());
        let at = self
            .pending
            .iter()
            .copied()
            .find(|&n| self.graph.nodes[n].id == ident);
        if let (Some(n), Some((p, v, _))) = (at, trace.open.take()) {
            trace.served.push((p, v, n));
        }
        if at.is_none() && !self.pending.is_empty() {
            trace.misjoins += 1;
            self.diverge(position);
        }
        if let Some(n) = at {
            if let Some(e) = self.fitting(n, frames, fits) {
                return Some(self.serve(n, e, &mut trace));
            }
            self.diverge(position);
        }
        let index = Arc::clone(&self.index);
        if let Some(cands) = index.get(&ident) {
            for &m in cands {
                if let Some(e) = self.fitting(m, frames, fits) {
                    trace.rescues += 1;
                    return Some(self.serve(m, e, &mut trace));
                }
            }
        }
        self.pending.clear();
        trace.node = None;
        None
    }

    fn divergence(&self) -> Option<Divergence> {
        self.divergence.clone()
    }

    fn longest(&self) -> usize {
        self.longest
    }
}

/// A resolver that serves nothing and records every draw's address: fresh
/// generation, with the structure of the run captured.
struct Recorder {
    addrs: Arc<Mutex<Vec<Addr>>>,
}

impl ExternalReplay for Recorder {
    fn resolve(
        &mut self,
        _stream: &[usize],
        _position: usize,
        frames: &[Frame],
        _fits: &dyn Fn(&ChoiceValue) -> bool,
    ) -> Option<ChoiceValue> {
        self.addrs.lock().unwrap().push(frames.to_vec());
        None
    }

    fn divergence(&self) -> Option<Divergence> {
        None
    }

    fn longest(&self) -> usize {
        0
    }
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

fn to_run(addrs: Vec<Addr>, values: Vec<ChoiceValue>) -> Option<Run> {
    if addrs.len() != values.len() {
        return None;
    }
    Some(Run {
        steps: addrs
            .into_iter()
            .zip(values)
            .map(|(addr, value)| Step { addr, value })
            .collect(),
    })
}

/// One fresh run of the body with its structure recorded.
fn discover(body: Body, hidden: &RefCell<Rng>, seed: u64) -> (bool, Option<Run>) {
    let addrs = Arc::new(Mutex::new(Vec::new()));
    let outcome = run(
        body,
        hidden,
        ReplayKind::External {
            resolver: Box::new(Recorder {
                addrs: Arc::clone(&addrs),
            }),
            budget: body.budget(),
        },
        seed,
    );
    let addrs = std::mem::take(&mut *addrs.lock().unwrap());
    (outcome.interesting, to_run(addrs, outcome.realized))
}

struct Replayed {
    outcome: ReplayOutcome,
    clean: bool,
    misjoined: bool,
    served: Watch,
    run: Option<Run>,
}

struct Prepared {
    graph: Arc<Graph>,
    index: Arc<HashMap<Ident, Vec<usize>>>,
}

fn prepare(g: &Graph) -> Prepared {
    let graph = Arc::new(g.clone());
    let index = Arc::new(graph.by_ident());
    Prepared { graph, index }
}

fn replay_graph(body: Body, hidden: &RefCell<Rng>, seed: u64, p: &Prepared) -> Replayed {
    let trace = Arc::new(Mutex::new(Trace::default()));
    let walk = Walk::new(
        Arc::clone(&p.graph),
        Arc::clone(&p.index),
        Arc::clone(&trace),
    );
    let budget = body.budget().max(walk.longest + EXTEND);
    let outcome = run(
        body,
        hidden,
        ReplayKind::External {
            resolver: Box::new(walk),
            budget,
        },
        seed,
    );
    let mut trace = std::mem::take(&mut *trace.lock().unwrap());
    if let Some((src, v, pending)) = trace.open.take() {
        if let Some(t) = pending.into_iter().find(|&t| p.graph.nodes[t].terminal()) {
            trace.served.push((src, v, t));
        }
    }
    let clean =
        outcome.divergence.is_none() && trace.node.is_some_and(|n| p.graph.nodes[n].terminal());
    let run = if outcome.interesting {
        to_run(trace.addrs, outcome.realized.clone())
    } else {
        None
    };
    Replayed {
        outcome,
        clean,
        misjoined: trace.misjoins > 0,
        served: trace.served,
        run,
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

fn parse_judge(name: &str) -> Option<Judge> {
    match name {
        "strict" => Some(Judge::Strict),
        "lenient" => Some(Judge::Lenient),
        "learn" => Some(Judge::Learn),
        _ => None,
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
    foreign: u64,
    unexercised: u64,
    misjoined: u64,
    start_ok: bool,
    max_edges: usize,
    max_nodes: usize,
    capped: bool,
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
    fn replay(&mut self, p: &Prepared) -> Replayed {
        self.stats.execs += 1;
        let r = replay_graph(self.body, self.hidden, (self.seed)(), p);
        self.stats.misjoined += r.misjoined as u64;
        r
    }

    /// Judge a candidate: `k` replays, stopping at the first that does not
    /// count as a failure — under `Strict` and `Learn` a failing replay that
    /// left the graph or ended off `END` is not one. Returns the verdict
    /// and, under `Learn`, the runs of failing replays the graph did not
    /// contain. A candidate whose edited edge (`watch`) no replay settled on
    /// is not accepted: the edit was never tested.
    fn accept(&mut self, cand: &Graph, watch: &[(usize, ChoiceValue, usize)]) -> (bool, Vec<Run>) {
        let p = prepare(cand);
        let mut learned = Vec::new();
        let mut exercised = watch.is_empty();
        for _ in 0..self.k {
            let r = self.replay(&p);
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
                    learned.extend(r.run);
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

    fn learn(&mut self, g: &mut Graph, runs: Vec<Run>) -> bool {
        let mut any = false;
        for run in runs {
            match g.walk_verdict(&run) {
                Walked::Whole => {
                    self.stats.redundant += 1;
                    continue;
                }
                Walked::Foreign => {
                    self.stats.foreign += 1;
                    continue;
                }
                Walked::Gap => {}
            }
            if std::env::var("TRACE_GRAFT").is_ok() {
                eprintln!("  graft: {}", run.describe());
            }
            self.stats.linked += g.graft(&run) as u64;
            self.stats.learned += 1;
            any = true;
        }
        any
    }

    fn consider(&mut self, g: &mut Graph, cand: Graph, mv: Move, watch: Watch) -> bool {
        if cand.key() >= g.key() {
            return false;
        }
        let (ok, learned) = self.accept(&cand, &watch);
        if ok {
            if std::env::var("TRACE_MOVES").is_ok() {
                let (e, n, _) = cand.key();
                eprintln!("  accept {mv:?} -> {n} nodes / {e} edges");
            }
            *g = cand;
            self.stats.accepts[mv as usize] += 1;
        } else {
            self.stats.rejects[mv as usize] += 1;
        }
        self.learn(g, learned);
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
            let p = prepare(g);
            let mut runs = Vec::new();
            for _ in 0..self.k {
                let r = self.replay(&p);
                if r.outcome.interesting && !r.clean {
                    runs.extend(r.run);
                }
            }
            let before = self.stats.learned;
            if !self.learn(g, runs) {
                break;
            }
            self.stats.warmed += self.stats.learned - before;
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
                let edge = g.nodes[n].edges[i].clone();
                if g.nodes[n].edges[..i].contains(&edge) {
                    *g = g.delete_edge(n, i);
                    self.stats.free += 1;
                    continue;
                }
                let keeps_a_way_on = g.nodes[n].edges.len() > 1 || g.nodes[n].terminal();
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
            let mut targets: Vec<usize> = g.nodes[n].edges.iter().map(|e| e.target).collect();
            targets.dedup();
            let into = g.edges_into(n);
            for t in targets {
                let watch = into.iter().map(|(p, v)| (*p, v.clone(), t)).collect();
                if self.consider(g, g.contract(n, t), Move::Contract, watch) {
                    break;
                }
            }
        }
    }

    fn pass_merge(&mut self, g: &mut Graph) {
        let order = g.bfs_order();
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
                if g.nodes[a].id != g.nodes[b].id || g.reaches(a, b) || g.reaches(b, a) {
                    continue;
                }
                let watch = g
                    .edges_into(b)
                    .into_iter()
                    .map(|(p, v)| (p, v, a))
                    .collect();
                if self.consider(g, g.merge_nodes(a, b), Move::MergeNodes, watch) {
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
                let t = g.nodes[n].edges[i].target;
                match g.nodes[n].edges[i].value.clone() {
                    ChoiceValue::Boolean(true) => {
                        let v = ChoiceValue::Boolean(false);
                        self.consider(
                            g,
                            g.set_value(n, i, v.clone()),
                            Move::Value,
                            vec![(n, v, t)],
                        );
                    }
                    ChoiceValue::Integer(x) => {
                        let x = x.to_i64().unwrap();
                        if x > 0 {
                            let int = |v: i64| ChoiceValue::Integer(BigInt::from(v));
                            if !self.consider(
                                g,
                                g.set_value(n, i, int(0)),
                                Move::Value,
                                vec![(n, int(0), t)],
                            ) {
                                let (mut lo, mut hi) = (0, x);
                                while hi - lo > 1 && !self.over() {
                                    let mid = lo + (hi - lo) / 2;
                                    if self.consider(
                                        g,
                                        g.set_value(n, i, int(mid)),
                                        Move::Value,
                                        vec![(n, int(mid), t)],
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
        self.learn(&mut g, learned);
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
            if g.key() == before && self.stats.learned == learned_before && !self.warm_up(&mut g) {
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
    let mut stats = GraphStats {
        nodes: graph.bfs_order().len(),
        edges: graph.edge_count(),
        paths: graph.count_paths(),
        enumerated: paths.len(),
        ..GraphStats::default()
    };
    let mut shapes: Vec<Vec<Addr>> = Vec::new();
    for p in &paths {
        match predicate(body, p) {
            Some(fail) => {
                if !fail {
                    stats.wrong += 1;
                }
                shapes.push(p.iter().map(|s| s.addr.clone()).collect());
            }
            None => stats.malformed += 1,
        }
    }
    shapes.sort();
    shapes.dedup();
    stats.shapes = shapes.len();
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
    misjoined: usize,
}

impl Cold {
    fn json(&self) -> String {
        format!(
            "[{},{},{},{}]",
            self.reproduced, self.diverged, self.clean, self.misjoined
        )
    }
}

fn cold(
    body: Body,
    hidden: &RefCell<Rng>,
    seed: &mut dyn FnMut() -> u64,
    graph: &Graph,
    r: usize,
) -> Cold {
    let p = prepare(graph);
    let mut c = Cold::default();
    for _ in 0..r {
        let rep = replay_graph(body, hidden, seed(), &p);
        c.reproduced += rep.outcome.interesting as usize;
        c.diverged += rep.outcome.divergence.is_some() as usize;
        c.clean += (rep.outcome.interesting && rep.clean) as usize;
        c.misjoined += rep.misjoined as usize;
    }
    c
}

fn env_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn run_trial(
    body: Body,
    confirm: usize,
    trial: u64,
    r: usize,
    ks: &[usize],
    judges: &[Judge],
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
        let (interesting, run) = discover(body, &hidden, next_seed());
        if interesting {
            t0 = run;
            break;
        }
    }
    let Some(t0) = t0 else {
        return false;
    };

    let mut poolall: Vec<Run> = vec![t0.clone()];
    let mut pool10: Vec<Run> = vec![t0.clone()];
    let mut confirm_fails = 0;
    let mut prepared = prepare(&Graph::from_runs(&pool10));
    for _ in 0..confirm {
        let rep = replay_graph(body, &hidden, next_seed(), &prepared);
        if rep.outcome.interesting {
            confirm_fails += 1;
            if let Some(run) = rep.run {
                if !poolall.contains(&run) {
                    poolall.push(run.clone());
                    if pool10.len() < 10 {
                        pool10.push(run);
                        prepared = prepare(&Graph::from_runs(&pool10));
                    }
                }
            }
        }
    }

    let trie = Graph::from_runs(&poolall);
    let starts: Vec<(&str, Graph)> = vec![
        ("t0", Graph::from_runs(&[t0])),
        ("exact", trie.merged_exact()),
        ("ident", trie.merged_ident()),
    ];

    let mut fields: Vec<String> = Vec::new();
    for (name, graph) in &starts {
        let stats = graph_stats(body, graph);
        let c = cold(body, &hidden, &mut next_seed, graph, r);
        fields.push(format!(
            "\"start-{name}\":{{{},\"cold\":{}}}",
            stats.json(),
            c.json()
        ));
    }
    for (name, graph) in &starts {
        for &judge in judges {
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
                    shrunk.dump();
                    for p in shrunk.paths(PATH_CAP) {
                        if predicate(body, &p) != Some(true) {
                            eprintln!("  wrong: {}", Run { steps: p }.describe());
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
                    "\"{name}-{}-{k}\":{{{},\"execs\":{},\"passes\":{},\"capped\":{},\"free\":{},\"learned\":{},\"warmed\":{},\"linked\":{},\"redundant\":{},\"foreign\":{},\"unexercised\":{},\"misjoined\":{},\"start_ok\":{},\"max_nodes\":{},\"max_edges\":{},\"accepts\":{{{}}},\"rejects\":{{{}}},\"cold\":{}}}",
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
                    stats.foreign,
                    stats.unexercised,
                    stats.misjoined,
                    stats.start_ok,
                    stats.max_nodes,
                    stats.max_edges,
                    accepts.join(","),
                    rejects.join(","),
                    c.json()
                ));
            }
        }
    }
    let (ideal_nodes, ideal_edges) = body.ideal();
    writeln!(
        out,
        "{{\"body\":\"{}\",\"k\":{},\"confirm\":{confirm},\"trial\":{trial},\"r\":{r},\"ideal\":[{ideal_nodes},{ideal_edges}],\"shapes_total\":{},\"discovery_attempts\":{discovery_attempts},\"confirm_fails\":{confirm_fails},\"poolall\":{},{}}}",
        body.name(),
        body.k,
        body.shapes_total(),
        poolall.len(),
        fields.join(",")
    )
    .unwrap();
    true
}

fn main() {
    let default_bodies = "block2,block4,block8,shift2,shift4,shift6,shift8,list4,list8,loop4,loop8";
    let bodies: Vec<Body> = std::env::var("BODIES")
        .unwrap_or_else(|_| default_bodies.to_string())
        .split(',')
        .map(|s| parse_body(s.trim()).unwrap_or_else(|| panic!("unknown body {s}")))
        .collect();
    let ks: Vec<usize> = std::env::var("KS")
        .unwrap_or_else(|_| "20".to_string())
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let judges: Vec<Judge> = std::env::var("JUDGES")
        .unwrap_or_else(|_| "strict,lenient,learn".to_string())
        .split(',')
        .map(|s| parse_judge(s.trim()).unwrap_or_else(|| panic!("unknown judge {s}")))
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
                if run_trial(*body, confirm, trial, r, &ks, &judges, &mut out) {
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
