//! Embedded tests for the graph walk in `src/native/core/replay.rs`
//! (decision 78), driven through `NativeTestCase`.

use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::{CloneRecord, NativeTestCase};
use crate::native::graph::{Run, Step};
use crate::native::rng::EngineRng;
use alloc::vec;

fn int(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

fn boolean(b: bool) -> ChoiceValue {
    ChoiceValue::Boolean(b)
}

fn clone_of(values: Vec<ChoiceValue>) -> ChoiceValue {
    ChoiceValue::Clone(Arc::new(CloneRecord::from_values(values)))
}

fn run(steps: &[(u64, ChoiceValue)]) -> Run {
    Run {
        steps: steps
            .iter()
            .map(|(label, value)| Step {
                addr: vec![(*label, 0)],
                value: value.clone(),
            })
            .collect(),
    }
}

fn graph(runs: &[Run]) -> Arc<Graph> {
    let mut g = Graph::new();
    for r in runs {
        g.insert(r);
    }
    Arc::new(g)
}

fn at(label: u64) -> Ident {
    Ident::At(vec![(label, 0)])
}

fn walk(graph: &Arc<Graph>) -> NativeTestCase {
    NativeTestCase::for_graph(Arc::clone(graph), EngineRng::seeded(5), 64).unwrap()
}

fn in_span<T>(tc: &mut NativeTestCase, label: u64, f: impl FnOnce(&mut NativeTestCase) -> T) -> T {
    tc.start_span(label);
    let v = f(tc);
    tc.stop_span(false);
    v
}

fn draw_int(tc: &mut NativeTestCase) -> i64 {
    tc.draw_integer::<i64>(0, 1000).unwrap()
}

fn draw_bool(tc: &mut NativeTestCase) -> bool {
    tc.weighted(0.5, None).unwrap()
}

fn diverged_at(stream: Vec<usize>, position: usize) -> Option<Divergence> {
    Some(Divergence { stream, position })
}

/// A coin whose hidden outcome decides which of two spans follows.
fn coin() -> Arc<Graph> {
    graph(&[
        run(&[(1, boolean(false)), (2, int(3))]),
        run(&[(1, boolean(false)), (3, int(5))]),
    ])
}

#[test]
fn a_tie_is_settled_by_the_identity_the_next_draw_reports() {
    let g = coin();
    let mut tc = walk(&g);
    assert!(!in_span(&mut tc, 1, draw_bool));
    assert!(!tc.ended_on_end());
    assert!(tc.settled_edges().is_empty());
    assert_eq!(in_span(&mut tc, 3, draw_int), 5);
    assert_eq!(tc.divergence(), None);
    assert!(tc.ended_on_end());
    let right = g.node(&at(3)).unwrap();
    assert_eq!(tc.settled_edges(), vec![(START, 1), (right, 0)]);
    assert!(tc.live_timelines().is_empty());
}

#[test]
fn a_misjoin_diverges_and_a_later_identity_rescues_the_walk() {
    let g = coin();
    let mut tc = walk(&g);
    in_span(&mut tc, 1, draw_bool);
    in_span(&mut tc, 4, draw_int);
    assert_eq!(tc.divergence(), diverged_at(vec![], 1));
    assert!(!tc.ended_on_end());
    assert!(tc.settled_edges().is_empty());
    assert_eq!(in_span(&mut tc, 3, draw_int), 5);
    assert_eq!(tc.divergence(), diverged_at(vec![], 1));
    assert!(tc.ended_on_end());
    let right = g.node(&at(3)).unwrap();
    assert_eq!(tc.settled_edges(), vec![(right, 0)]);
}

#[test]
fn a_misfit_diverges_and_draws_at_random_until_rescued() {
    let g = coin();
    let mut tc = walk(&g);
    in_span(&mut tc, 1, draw_int);
    assert_eq!(tc.divergence(), diverged_at(vec![], 0));
    assert_eq!(in_span(&mut tc, 3, draw_int), 5);
    assert!(tc.ended_on_end());
}

#[test]
fn a_run_past_the_end_diverges() {
    let g = coin();
    let mut tc = walk(&g);
    in_span(&mut tc, 1, draw_bool);
    in_span(&mut tc, 2, draw_int);
    assert!(tc.ended_on_end());
    in_span(&mut tc, 9, draw_int);
    assert_eq!(tc.divergence(), diverged_at(vec![], 2));
    assert!(!tc.ended_on_end());
    let left = g.node(&at(2)).unwrap();
    assert_eq!(tc.settled_edges(), vec![(START, 0)]);
    assert_eq!(g.nodes()[left].edges.len(), 1);
}

#[test]
fn the_empty_graph_diverges_at_the_first_draw() {
    let mut tc = walk(&Arc::new(Graph::new()));
    in_span(&mut tc, 1, draw_int);
    assert_eq!(tc.divergence(), diverged_at(vec![], 0));
    assert!(!tc.ended_on_end());
}

/// A clone whose record decides which span follows.
fn cloned() -> Arc<Graph> {
    graph(&[
        run(&[(1, clone_of(vec![int(1)])), (2, int(7))]),
        run(&[(1, clone_of(vec![int(2)])), (3, int(8))]),
    ])
}

#[test]
fn a_clones_records_are_a_live_set_whose_agreement_settles_the_tie() {
    let g = cloned();
    let mut tc = walk(&g);
    let child = in_span(&mut tc, 1, |tc| tc.clone_stream().unwrap());
    assert_eq!(draw_int(&mut child.lock()), 1);
    assert_eq!(in_span(&mut tc, 2, draw_int), 7);
    assert_eq!(tc.divergence(), None);
    assert!(tc.ended_on_end());
    let left = g.node(&at(2)).unwrap();
    assert_eq!(tc.settled_edges(), vec![(START, 0), (left, 0)]);
}

#[test]
fn a_parent_leaving_the_live_records_states_misjoins() {
    let g = cloned();
    let mut tc = walk(&g);
    let child = in_span(&mut tc, 1, |tc| tc.clone_stream().unwrap());
    draw_int(&mut child.lock());
    assert_eq!(in_span(&mut tc, 3, draw_int), 8);
    assert_eq!(tc.divergence(), diverged_at(vec![], 1));
}

#[test]
fn a_child_divergence_is_the_walks() {
    let g = cloned();
    let mut tc = walk(&g);
    let child = in_span(&mut tc, 1, |tc| tc.clone_stream().unwrap());
    draw_bool(&mut child.lock());
    assert_eq!(tc.divergence(), diverged_at(vec![0], 0));
    in_span(&mut tc, 2, draw_int);
    assert_eq!(tc.divergence(), diverged_at(vec![0], 0));
    assert!(tc.ended_on_end());
}

#[test]
fn a_clone_the_graph_has_no_record_for_diverges_and_draws_at_random() {
    let g = coin();
    let mut tc = walk(&g);
    let child = in_span(&mut tc, 1, |tc| tc.clone_stream().unwrap());
    assert_eq!(tc.divergence(), diverged_at(vec![], 0));
    draw_int(&mut child.lock());
    assert_eq!(child.lock().nodes.len(), 1);
    assert_eq!(in_span(&mut tc, 3, draw_int), 5);
}

#[test]
fn a_clone_at_an_unknown_identity_diverges() {
    let g = cloned();
    let mut tc = walk(&g);
    in_span(&mut tc, 1, |tc| tc.clone_stream().unwrap());
    in_span(&mut tc, 9, |tc| tc.clone_stream().unwrap());
    assert_eq!(tc.divergence(), diverged_at(vec![], 1));
    assert!(!tc.ended_on_end());
}

#[test]
fn a_run_ending_on_a_clone_settles_it() {
    let g = graph(&[run(&[(1, clone_of(vec![int(1)]))])]);
    let mut tc = walk(&g);
    let child = in_span(&mut tc, 1, |tc| tc.clone_stream().unwrap());
    assert_eq!(draw_int(&mut child.lock()), 1);
    assert!(tc.ended_on_end());
    assert_eq!(tc.settled_edges(), vec![(START, 0)]);
    assert_eq!(tc.divergence(), None);
}
