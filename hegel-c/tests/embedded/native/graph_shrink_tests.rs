//! Embedded tests for `src/native/graph_shrink.rs`: the graph shrinker
//! driven against small test bodies through `NativeTestCase::for_graph`.

use super::*;
use crate::exchange::drive_no_yield;
use crate::native::core::{CloneRecord, EngineError, NativeTestCase, RealizedStream, Span, Status};
use crate::native::graph::{START, Step};
use crate::native::intervalsets::IntervalSet;
use crate::native::rng::EngineRng;
use alloc::vec;
use core::time::Duration;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn coin(&mut self, p: f64) -> bool {
        (self.next() as f64) / ((1u64 << 31) as f64) < p
    }
}

type Body = fn(&mut NativeTestCase, &mut Lcg) -> Option<bool>;

fn draw<T>(
    tc: &mut NativeTestCase,
    label: u64,
    f: impl FnOnce(&mut NativeTestCase) -> Result<T, EngineError>,
) -> Option<T> {
    tc.start_span(label);
    let v = f(tc).ok();
    tc.stop_span(false);
    v
}

fn int(tc: &mut NativeTestCase, label: u64, max: i64) -> Option<i64> {
    draw(tc, label, |tc| tc.draw_integer::<i64>(0, max))
}

fn value(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

struct Probe {
    body: Body,
    seed: u64,
    min_fails: u64,
    charges: Vec<bool>,
    adopted: Vec<(usize, f64)>,
}

impl Probe {
    fn new(body: Body) -> Probe {
        Probe {
            body,
            seed: 1,
            min_fails: nd::GAUNTLET_MIN_FAILS,
            charges: Vec::new(),
            adopted: Vec::new(),
        }
    }

    fn run(&mut self, graph: Arc<Graph>, max_size: usize) -> Outcome {
        let mut tc =
            NativeTestCase::for_graph(graph, EngineRng::seeded(self.seed), max_size).unwrap();
        let mut lcg = Lcg(self.seed.wrapping_mul(7919));
        self.seed += 1;
        let failed = (self.body)(&mut tc, &mut lcg) == Some(true);
        tc.conclude(Status::Valid, None);
        tc.freeze();
        tc.reassemble();
        Outcome {
            failed,
            divergence: tc.divergence().is_some(),
            ended: tc.ended_on_end(),
            settled: tc.settled_edges(),
            nodes: tc.nodes.clone(),
            spans: tc.spans.clone().into_vec(),
        }
    }
}

impl GraphProbe for Probe {
    fn replay<'s>(&'s mut self, graph: Arc<Graph>, max_size: usize) -> ProbeFuture<'s> {
        Box::pin(async move { Ok(self.run(graph, max_size)) })
    }

    fn charge(&mut self, _: f64, drive: bool) -> u64 {
        self.charges.push(drive);
        self.min_fails
    }

    fn adopted(
        &mut self,
        graph: &Graph,
        _: (&[ChoiceNode], &[Span]),
        anchor: f64,
        _: usize,
    ) -> Result<(), RunError> {
        self.adopted.push((graph.edge_count(), anchor));
        Ok(())
    }
}

fn run(steps: &[(&[(u64, usize)], ChoiceValue)]) -> Run {
    Run {
        steps: steps
            .iter()
            .map(|(addr, value)| Step {
                addr: addr.to_vec(),
                value: value.clone(),
            })
            .collect(),
    }
}

/// A shrinker over the graph of `runs`, its witness realized by replaying
/// the graph until a clean failure.
fn start(body: Body, runs: &[Run], anchor: f64) -> (GraphShrinker, Probe) {
    let mut graph = Graph::new();
    for r in runs {
        graph.insert(r);
    }
    let mut probe = Probe::new(body);
    let shared = Arc::new(graph.clone());
    let witness = (0..20)
        .map(|_| probe.run(Arc::clone(&shared), 64))
        .find(Outcome::clean)
        .unwrap();
    (
        GraphShrinker::new(graph, witness.nodes, witness.spans, anchor),
        probe,
    )
}

fn shrink(shrinker: &mut GraphShrinker, probe: &mut Probe) {
    drive_no_yield(shrinker.shrink(probe)).unwrap();
}

fn ints(nodes: &[ChoiceNode]) -> Vec<i64> {
    nodes
        .iter()
        .map(|n| match n.value() {
            ChoiceValue::Integer(v) => v.to_i64().unwrap(),
            other => panic!("not an integer: {other:?}"),
        })
        .collect()
}

const A: &[(u64, usize)] = &[(1, 0)];
const B: &[(u64, usize)] = &[(2, 0)];
const C: &[(u64, usize)] = &[(3, 0)];

fn two_ints(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let a = int(tc, 1, 100)?;
    let b = int(tc, 2, 100)?;
    Some(a + b >= 10)
}

#[test]
fn values_shrink_by_binary_search_toward_the_simplest_under_the_gauntlet() {
    let (mut shrinker, mut probe) = start(two_ints, &[run(&[(A, value(7)), (B, value(8))])], 0.5);
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![2, 8]);
    assert_eq!(shrinker.graph().edge_count(), 2);
    assert_eq!(probe.adopted.len(), 2);
    assert!(probe.adopted.iter().all(|&(edges, _)| edges == 2));
    assert!(shrinker.anchor() > 0.5 && shrinker.anchor() <= nd::anchor_ceiling());
    assert!(probe.charges.contains(&false) && probe.charges.contains(&true));
    assert!(!shrinker.timed_out);
    assert!(shrinker.replays() > 0);
}

#[test]
fn an_accept_tops_its_ledger_up_before_seeding_the_anchor() {
    let (mut shrinker, mut probe) = start(two_ints, &[run(&[(A, value(7)), (B, value(8))])], 0.5);
    shrink(&mut shrinker, &mut probe);
    assert_eq!(probe.adopted[0].1, nd::anchor_ceiling());
    assert_eq!(shrinker.anchor(), nd::anchor_ceiling());
    assert!(shrinker.replays() >= probe.adopted.len() as u64 * nd::ANCHOR_SEED_RUNS);
}

fn hidden_coin(tc: &mut NativeTestCase, lcg: &mut Lcg) -> Option<bool> {
    int(tc, 1, 10)?;
    let y = if lcg.coin(0.5) {
        int(tc, 2, 10)?
    } else {
        int(tc, 3, 10)?
    };
    Some(y >= 3)
}

#[test]
fn a_hidden_coins_arms_shrink_together_and_the_tie_survives() {
    let (mut shrinker, mut probe) = start(
        hidden_coin,
        &[
            run(&[(A, value(5)), (B, value(6))]),
            run(&[(A, value(5)), (C, value(7))]),
        ],
        0.5,
    );
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![0, 3]);
    let graph = shrinker.graph();
    let start = &graph.nodes()[START].edges;
    assert_eq!(start.len(), 2);
    assert!(start.iter().all(|e| e.value == value(0)));
    assert_eq!(graph.edge_count(), 4);
    for n in graph.reachable() {
        for e in &graph.nodes()[n].edges {
            if e.addr != A {
                assert_eq!(e.value, value(3));
            }
        }
    }
}

fn list(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let n = int(tc, 1, 5)?;
    let mut failed = false;
    for _ in 0..n {
        failed |= int(tc, 2, 10)? >= 5;
    }
    Some(failed)
}

#[test]
fn a_list_shrinks_to_the_one_element_that_fails() {
    let (mut shrinker, mut probe) = start(
        list,
        &[run(&[
            (A, value(3)),
            (B, value(1)),
            (&[(2, 1)], value(7)),
            (&[(2, 2)], value(2)),
        ])],
        0.5,
    );
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![1, 5]);
    assert_eq!(shrinker.graph().edge_count(), 2);
    assert_eq!(shrinker.graph().nodes().len(), 3);
}

fn flaky(tc: &mut NativeTestCase, lcg: &mut Lcg) -> Option<bool> {
    let x = int(tc, 1, 10)?;
    Some(x >= 3 && lcg.coin(0.7))
}

#[test]
fn the_confirmation_sweep_finishes_what_a_fast_miss_left() {
    let (mut shrinker, mut probe) = start(flaky, &[run(&[(A, value(7))])], 0.3);
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![3]);
    assert!(probe.charges.contains(&true));
}

#[test]
fn an_expired_deadline_stops_the_shrink_before_any_replay() {
    let (mut shrinker, mut probe) = start(two_ints, &[run(&[(A, value(7)), (B, value(8))])], 0.5);
    shrinker.deadline = Some(crate::sys::Instant::now().unwrap() - Duration::from_secs(1));
    shrink(&mut shrinker, &mut probe);
    assert!(shrinker.timed_out);
    assert_eq!(shrinker.replays(), 0);
    assert_eq!(ints(shrinker.witness().0), vec![7, 8]);
    assert!(probe.adopted.is_empty());
}

fn early_exit(tc: &mut NativeTestCase, lcg: &mut Lcg) -> Option<bool> {
    let x = int(tc, 1, 10)?;
    if lcg.coin(0.5) {
        return Some(x >= 3);
    }
    int(tc, 2, 10)?;
    Some(x >= 3)
}

#[test]
fn a_failing_run_the_graph_cannot_produce_is_grafted_unless_foreign() {
    let (mut shrinker, _) = start(early_exit, &[run(&[(A, value(5)), (B, value(4))])], 0.5);
    assert_eq!(shrinker.graph().edge_count(), 2);
    shrinker.graft(&run(&[(A, value(9)), (B, value(2))]));
    assert_eq!(
        shrinker.graph().edge_count(),
        2,
        "foreign at the first draw: the walk serves 5 there"
    );
    shrinker.graft(&run(&[(A, value(5)), (B, value(1))]));
    assert_eq!(
        shrinker.graph().edge_count(),
        2,
        "foreign at the second draw"
    );
    shrinker.graft(&run(&[(A, value(5))]));
    assert_eq!(
        shrinker.graph().edge_count(),
        3,
        "the early exit is a gap: a tie at the start"
    );
    let start = &shrinker.graph().nodes()[START].edges;
    assert!(start.iter().all(|e| e.value == value(5)));
    assert!(start.iter().any(|e| e.target == crate::native::graph::END));
}

#[test]
fn an_early_exits_shrink_keeps_no_value_the_walk_never_serves() {
    let (mut shrinker, mut probe) = start(early_exit, &[run(&[(A, value(5)), (B, value(4))])], 0.5);
    shrink(&mut shrinker, &mut probe);
    let witness = ints(shrinker.witness().0);
    assert_eq!(witness[0], 3);
    assert!(witness.len() <= 2);
    let graph = shrinker.graph();
    assert!(graph.edge_count() <= 3);
    let start = &graph.nodes()[START].edges;
    assert!(
        start.iter().all(|e| e.value == value(3)),
        "every start edge is the shrunk value: {:?}",
        graph.nodes()
    );
}

fn float_body(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let f = draw(tc, 1, |tc| tc.draw_float(0.0, 100.0, false, false, 5e-324))?;
    Some(f >= 2.5)
}

#[test]
fn floats_shrink_to_their_integer_part() {
    let (mut shrinker, mut probe) = start(float_body, &[run(&[(A, ChoiceValue::Float(7.5))])], 0.5);
    shrink(&mut shrinker, &mut probe);
    assert_eq!(shrinker.witness().0[0].value(), ChoiceValue::Float(7.0));
}

fn bytes_body(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let b = draw(tc, 1, |tc| tc.draw_bytes(0, 8))?;
    Some(b.len() >= 3 && b[2] == 5)
}

#[test]
fn sequences_halve_then_drop_then_zero() {
    let (mut shrinker, mut probe) = start(
        bytes_body,
        &[run(&[(A, ChoiceValue::Bytes(vec![3, 4, 5, 6]))])],
        0.5,
    );
    shrink(&mut shrinker, &mut probe);
    assert_eq!(
        shrinker.witness().0[0].value(),
        ChoiceValue::Bytes(vec![0, 0, 5])
    );
}

fn boolean_body(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    draw(tc, 1, |tc| tc.weighted(0.5, None))
}

fn huge_body(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let x = draw(tc, 1, |tc| tc.draw_integer::<u128>(0, u128::MAX))?;
    Some(x >= 1 << 100)
}

#[test]
fn a_boolean_or_an_integer_past_i128_has_only_the_simplest_to_try() {
    let (mut shrinker, mut probe) = start(
        boolean_body,
        &[run(&[(A, ChoiceValue::Boolean(true))])],
        0.5,
    );
    shrink(&mut shrinker, &mut probe);
    assert_eq!(shrinker.witness().0[0].value(), ChoiceValue::Boolean(true));
    let huge = ChoiceValue::Integer(BigInt::from(u128::MAX));
    let (mut shrinker, mut probe) = start(huge_body, &[run(&[(A, huge.clone())])], 0.5);
    shrink(&mut shrinker, &mut probe);
    assert_eq!(shrinker.witness().0[0].value(), huge);
    assert!(probe.adopted.is_empty());
}

fn clone_body(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let child = draw(tc, 1, |tc| tc.clone_stream())?;
    let x = child.lock().draw_integer::<i64>(0, 10).ok()?;
    Some(x >= 3)
}

#[test]
fn a_clones_record_is_not_shrunk_in_this_cut() {
    let record = ChoiceValue::Clone(Arc::new(CloneRecord::from_values(vec![value(5)])));
    let (mut shrinker, mut probe) = start(clone_body, &[run(&[(A, record)])], 0.5);
    shrink(&mut shrinker, &mut probe);
    assert!(probe.adopted.is_empty());
    assert!(shrinker.constraints.is_empty());
    assert_eq!(shrinker.graph().edge_count(), 1);
}

fn string_body(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let s = draw(tc, 1, |tc| {
        tc.draw_string(
            Arc::new(IntervalSet::new(vec![(b'a' as u32, b'c' as u32)]).unwrap()),
            0,
            8,
        )
    })?;
    let chars: Vec<char> = s.chars().collect();
    Some(chars.len() >= 2 && chars[0] == 'c' && chars[1] != 'a')
}

#[test]
fn strings_shrink_like_bytes_toward_their_simplest_codepoint() {
    let cb = |s: &str| ChoiceValue::String(s.chars().map(|c| c as u32).collect());
    let (mut shrinker, mut probe) = start(string_body, &[run(&[(A, cb("cbcb"))])], 0.5);
    shrink(&mut shrinker, &mut probe);
    assert_eq!(shrinker.witness().0[0].value(), cb("cb"));
}

#[test]
fn a_value_edit_no_clean_replay_settled_on_is_rejected() {
    let (mut shrinker, mut probe) = start(two_ints, &[run(&[(A, value(7)), (B, value(8))])], 0.5);
    let candidate = (**shrinker.graph()).clone();
    let edit = Edit::Value {
        node: START,
        addr: C.to_vec(),
        value: value(9),
    };
    let verdict = drive_no_yield(shrinker.judge(&mut probe, candidate, edit)).unwrap();
    assert!(matches!(verdict, Verdict::Rejected));
    assert_eq!(shrinker.replays(), nd::GAUNTLET_MIN_FAILS);
}

#[test]
fn a_value_edit_for_a_draw_the_graph_no_longer_has_is_skipped() {
    let (mut shrinker, mut probe) = start(two_ints, &[run(&[(A, value(7)), (B, value(8))])], 0.5);
    let data = shrinker
        .constraints
        .get(&(Ident::Start, A.to_vec()))
        .unwrap()
        .clone();
    let mut key = EdgeKey {
        from: Ident::At(C.to_vec()),
        addr: A.to_vec(),
        value: value(7),
        to: Ident::End,
    };
    let moved = drive_no_yield(shrinker.try_value(&mut probe, &mut key, &data, value(1))).unwrap();
    assert!(!moved);
    assert_eq!(shrinker.replays(), 0);
}

#[test]
fn adopting_keeps_a_smaller_standing_witness_the_new_graph_still_walks() {
    let (mut shrinker, mut probe) = start(early_exit, &[run(&[(A, value(5))])], 0.5);
    assert_eq!(ints(shrinker.witness().0), vec![5]);
    let mut graph = (**shrinker.graph()).clone();
    graph.insert(&run(&[(A, value(5)), (B, value(4))]));
    let shared = Arc::new(graph.clone());
    let long = (0..40)
        .map(|_| probe.run(Arc::clone(&shared), 8))
        .find(|o| o.nodes.len() == 2)
        .unwrap();
    let long = Outcome {
        failed: true,
        divergence: false,
        ended: true,
        ..long
    };
    shrinker
        .adopt(&mut probe, graph, long.clone(), 0.6)
        .unwrap();
    assert_eq!(ints(shrinker.witness().0), vec![5]);
    assert_eq!(shrinker.anchor(), 0.6);
    let only_long = Graph::from_run(&run(&[(A, value(5)), (B, value(4))]));
    shrinker.adopt(&mut probe, only_long, long, 0.2).unwrap();
    assert_eq!(ints(shrinker.witness().0).len(), 2);
    assert_eq!(shrinker.anchor(), 0.6);
    assert_eq!(probe.adopted.len(), 2);
}

#[test]
fn constraints_are_learned_for_every_draw_but_clones() {
    let (mut shrinker, _) = start(two_ints, &[run(&[(A, value(7)), (B, value(8))])], 0.5);
    assert_eq!(shrinker.constraints.len(), 2);
    let clone = ChoiceNode::clone_stream(Arc::new(RealizedStream::empty()), false);
    let span = Span {
        start: 0,
        end: 1,
        label: 9,
        depth: 0,
        parent: None,
        discarded: false,
    };
    shrinker.learn_constraints(&[clone], &[span]);
    assert_eq!(shrinker.constraints.len(), 2);
}

fn counted_list(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let n = int(tc, 1, 5)?;
    let mut bug = false;
    for _ in 0..n {
        bug |= int(tc, 2, 20)? >= 10;
    }
    Some(bug)
}

#[test]
fn a_counted_list_shrinks_by_deleting_elements_and_lowering_the_count() {
    let (mut shrinker, mut probe) = start(
        counted_list,
        &[run(&[
            (A, value(3)),
            (&[(2, 0)], value(0)),
            (&[(2, 1)], value(0)),
            (&[(2, 2)], value(10)),
        ])],
        0.5,
    );
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![1, 10]);
    assert_eq!(shrinker.graph().edge_count(), 2);
}

fn pairs(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let n = int(tc, 1, 3)?;
    let mut bug = false;
    for _ in 0..n {
        tc.start_span(5);
        let first = int(tc, 2, 20);
        let second = int(tc, 3, 20);
        tc.stop_span(false);
        first?;
        bug |= second? >= 10;
    }
    Some(bug)
}

#[test]
fn deleting_a_span_renumbers_the_siblings_after_it() {
    let (mut shrinker, mut probe) = start(
        pairs,
        &[run(&[
            (A, value(2)),
            (&[(5, 0), (2, 0)], value(0)),
            (&[(5, 0), (3, 0)], value(0)),
            (&[(5, 1), (2, 0)], value(0)),
            (&[(5, 1), (3, 0)], value(10)),
        ])],
        0.5,
    );
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![1, 0, 10]);
    let (nodes, spans) = shrinker.witness();
    let run = Run::from_nodes(nodes, spans);
    assert_eq!(run.steps[2].addr, vec![(5, 0), (3, 0)]);
}

fn four_ints(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let a = int(tc, 1, 100)?;
    let b = int(tc, 2, 100)?;
    let c = int(tc, 3, 100)?;
    let d = int(tc, 4, 100)?;
    Some(a + b + c + d >= 10)
}

#[test]
fn a_span_the_test_always_draws_is_retried_with_at_most_the_nearest_count_lowerings() {
    let (mut shrinker, mut probe) = start(
        four_ints,
        &[run(&[
            (A, value(5)),
            (B, value(5)),
            (C, value(5)),
            (&[(4, 0)], value(10)),
        ])],
        0.5,
    );
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![0, 0, 0, 10]);
    assert_eq!(shrinker.graph().edge_count(), 4);
}

fn one_int(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    Some(int(tc, 1, 100)? >= 10)
}

#[test]
fn the_only_draw_is_never_proposed_for_deletion() {
    let (mut shrinker, mut probe) = start(one_int, &[run(&[(A, value(50))])], 0.5);
    shrink(&mut shrinker, &mut probe);
    assert_eq!(ints(shrinker.witness().0), vec![10]);
}

#[test]
fn spans_are_ordered_last_starting_first_then_outermost_first() {
    let r = run(&[
        (A, value(1)),
        (&[(5, 0), (2, 0)], value(0)),
        (&[(5, 0), (3, 0)], value(0)),
        (&[(5, 1), (2, 0)], value(0)),
    ]);
    let spans: Vec<Addr> = spans_of(&r).into_iter().map(|(s, _)| s).collect();
    assert_eq!(
        spans,
        vec![
            vec![(5, 1)],
            vec![(5, 1), (2, 0)],
            vec![(5, 0), (3, 0)],
            vec![(5, 0)],
            vec![(5, 0), (2, 0)],
            vec![(1, 0)],
        ]
    );
    let shorter = without_span(&r, &[(5, 0)]);
    assert_eq!(shorter.steps.len(), 2);
    assert_eq!(shorter.steps[1].addr, vec![(5, 0), (2, 0)]);
    let inner = without_span(&r, &[(5, 0), (2, 0)]);
    assert_eq!(inner.steps.len(), 3);
    assert_eq!(inner.steps[1].addr, vec![(5, 0), (3, 0)]);
    assert_eq!(inner.steps[2].addr, vec![(5, 1), (2, 0)]);
}

fn wrapped_then_tail(tc: &mut NativeTestCase, lcg: &mut Lcg) -> Option<bool> {
    if lcg.0 % 2 == 1 {
        tc.start_span(1);
        let v = int(tc, 2, 10);
        tc.stop_span(false);
        v?;
    }
    let c = int(tc, 3, 10)?;
    Some(c >= 3)
}

#[test]
fn a_span_the_test_stops_drawing_is_deleted_outright_and_its_inner_span_skipped() {
    let (mut shrinker, mut probe) = start(
        wrapped_then_tail,
        &[run(&[(&[(1, 0), (2, 0)], value(5)), (C, value(7))])],
        0.1,
    );
    assert_eq!(ints(shrinker.witness().0), vec![5, 7]);
    probe.min_fails = 1;
    probe.seed = 2;
    assert!(drive_no_yield(shrinker.span_pass(&mut probe)).unwrap());
    assert_eq!(ints(shrinker.witness().0), vec![7]);
    let graph = shrinker.graph();
    assert_eq!(graph.edge_count(), 1);
    assert_eq!(graph.nodes()[START].edges[0].addr, C.to_vec());
    assert_eq!(probe.adopted.len(), 1);
    assert_eq!(probe.adopted[0].0, 1);
}

fn flag_then_pair(tc: &mut NativeTestCase, _: &mut Lcg) -> Option<bool> {
    let f = draw(tc, 1, |tc| tc.weighted(0.5, None))?;
    let a = int(tc, 2, 10)?;
    let b = int(tc, 3, 10)?;
    Some(f && a + b >= 10)
}

#[test]
fn only_integer_draws_are_lowered_for_a_span_deletion() {
    let (mut shrinker, mut probe) = start(
        flag_then_pair,
        &[run(&[
            (A, ChoiceValue::Boolean(true)),
            (B, value(5)),
            (C, value(5)),
        ])],
        0.5,
    );
    assert!(!drive_no_yield(shrinker.span_pass(&mut probe)).unwrap());
    assert_eq!(shrinker.witness().0.len(), 3);
    assert!(probe.adopted.is_empty());
    assert!(shrinker.replays() > 0);
}
