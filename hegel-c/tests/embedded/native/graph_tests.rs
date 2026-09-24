//! Embedded tests for `src/native/graph.rs`.

use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::choices::{BooleanChoice, IntegerChoice, RealizedStream};
use crate::native::core::{CloneRecord, MAX_CLONE_DEPTH};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

fn int(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

fn step(addr: &[Frame], value: ChoiceValue) -> Step {
    Step {
        addr: addr.to_vec(),
        value,
    }
}

fn run(steps: Vec<Step>) -> Run {
    Run { steps }
}

const COIN: &[Frame] = &[(1, 0), (10, 0)];
const LEFT: &[Frame] = &[(2, 0), (11, 0)];
const RIGHT: &[Frame] = &[(3, 0), (11, 0)];

fn left_run(coin: bool, v: i64) -> Run {
    run(vec![
        step(COIN, ChoiceValue::Boolean(coin)),
        step(LEFT, int(v)),
    ])
}

fn right_run(coin: bool, v: i64) -> Run {
    run(vec![
        step(COIN, ChoiceValue::Boolean(coin)),
        step(RIGHT, int(v)),
    ])
}

fn at(frames: &[Frame]) -> Ident {
    Ident::At(frames.to_vec())
}

fn span(start: usize, end: usize, label: u64, parent: Option<usize>) -> Span {
    Span {
        start,
        end,
        label,
        depth: 0,
        parent,
        discarded: false,
    }
}

#[test]
fn draw_addresses_follow_open_spans_with_sibling_ordinals() {
    let spans = vec![
        span(0, 2, 1, None),
        span(0, 1, 17, Some(0)),
        span(1, 1, 3, Some(0)),
        span(1, 2, 17, Some(0)),
        span(2, 3, 1, None),
        span(2, 3, 18, Some(4)),
        span(9, 9, 1, None),
    ];
    assert_eq!(
        draw_addresses(&spans, 3),
        vec![
            vec![(1, 0), (17, 0)],
            vec![(1, 0), (17, 1)],
            vec![(1, 1), (18, 0)],
        ]
    );
}

#[test]
fn run_from_nodes_pairs_values_with_addresses() {
    let nodes = vec![
        ChoiceNode::boolean(BooleanChoice { p: 0.5 }, true, false),
        ChoiceNode::integer(
            IntegerChoice {
                min_value: BigInt::from(0),
                max_value: BigInt::from(9),
                shrink_towards: BigInt::from(0),
            },
            BigInt::from(4),
            false,
        ),
    ];
    let spans = vec![span(0, 1, 20, None), span(1, 2, 17, None)];
    let r = Run::from_nodes(&nodes, &spans);
    assert_eq!(
        r,
        run(vec![
            step(&[(20, 0)], ChoiceValue::Boolean(true)),
            step(&[(17, 0)], int(4)),
        ])
    );
    assert_eq!(r.values(), vec![ChoiceValue::Boolean(true), int(4)]);
    assert_eq!(r.idents(), vec![at(&[(17, 0)]), Ident::End]);
    assert!(Run::default().idents().is_empty());
}

#[test]
fn identity_is_the_prefix_through_the_first_new_frame() {
    assert_eq!(ident_after(COIN, LEFT), at(&[(2, 0)]));
    assert_eq!(
        ident_after(&[(1, 0), (17, 0)], &[(1, 0), (17, 1)]),
        at(&[(1, 0), (17, 1)])
    );
    assert_eq!(
        ident_after(&[(1, 0), (17, 0)], &[(1, 0), (2, 0), (17, 0)]),
        at(&[(1, 0), (2, 0)])
    );
    assert_eq!(ident_after(&[(1, 0), (17, 0)], &[(1, 0)]), at(&[(1, 0)]));
    assert_eq!(ident_after(COIN, &[]), at(&[]));
    assert_eq!(ident_before(None, COIN), Ident::Start);
    assert_eq!(ident_before(Some(COIN), RIGHT), at(&[(3, 0)]));
}

#[test]
fn empty_graph_has_start_and_end() {
    let g = Graph::default();
    assert_eq!(g.nodes().len(), 2);
    assert_eq!(g.nodes()[START].ident, Ident::Start);
    assert_eq!(g.nodes()[END].ident, Ident::End);
    assert_eq!(g.node(&Ident::Start), Some(START));
    assert_eq!(g.node(&Ident::End), Some(END));
    assert_eq!(g.node(&at(LEFT)), None);
    assert_eq!(g.edge_count(), 0);
    assert!(!Graph::new().insert(&Run::default()));
}

#[test]
fn inserting_runs_merges_by_identity_and_records_ties() {
    let mut g = Graph::from_run(&left_run(false, 3));
    assert_eq!(g.nodes().len(), 3);
    assert_eq!(g.edge_count(), 2);
    assert!(!g.insert(&left_run(false, 3)));
    assert!(g.insert(&right_run(false, 5)));
    let start = &g.nodes()[START].edges;
    assert_eq!(start.len(), 2);
    assert_eq!(start[0].value, start[1].value);
    assert_eq!(start[0].addr, start[1].addr);
    assert_ne!(start[0].target, start[1].target);
    assert_eq!(g.nodes()[start[0].target].ident, at(&[(2, 0)]));
    assert_eq!(g.nodes()[start[1].target].ident, at(&[(3, 0)]));
    assert!(g.insert(&left_run(false, 7)));
    let left = g.node(&at(&[(2, 0)])).unwrap();
    assert_eq!(g.nodes()[left].edges.len(), 2);
    assert_eq!(g.edge_count(), 5);
}

#[test]
fn walk_verdicts() {
    let mut g = Graph::from_run(&left_run(false, 3));
    g.insert(&right_run(false, 5));
    assert_eq!(g.walk_verdict(&left_run(false, 3)), Walked::Whole);
    assert_eq!(g.walk_verdict(&right_run(false, 5)), Walked::Whole);
    assert_eq!(g.walk_verdict(&left_run(true, 3)), Walked::Foreign);
    assert_eq!(g.walk_verdict(&left_run(false, 4)), Walked::Foreign);
    assert_eq!(g.walk_verdict(&right_run(false, 6)), Walked::Foreign);
    assert_eq!(
        g.walk_verdict(&run(vec![
            step(COIN, ChoiceValue::Boolean(false)),
            step(&[(4, 0)], int(3)),
        ])),
        Walked::Gap
    );
    assert_eq!(
        g.walk_verdict(&run(vec![
            step(COIN, ChoiceValue::Boolean(false)),
            step(&[(2, 0), (12, 0)], int(3)),
        ])),
        Walked::Gap
    );
    assert_eq!(
        g.walk_verdict(&run(vec![step(COIN, int(0)), step(LEFT, int(3))])),
        Walked::Gap
    );
    assert_eq!(g.walk_verdict(&Run::default()), Walked::Gap);
    assert_eq!(
        g.walk_verdict(&run(vec![
            step(COIN, ChoiceValue::Boolean(true)),
            step(&[(4, 0)], int(3)),
        ])),
        Walked::Foreign,
        "the walk serves false before it reaches the state the run has and the graph lacks"
    );
}

#[test]
fn walk_verdict_gap_when_the_value_leads_elsewhere() {
    let g = Graph::from_run(&run(vec![
        step(COIN, ChoiceValue::Boolean(false)),
        step(LEFT, int(3)),
        step(RIGHT, int(7)),
    ]));
    assert_eq!(g.walk_verdict(&right_run(false, 7)), Walked::Gap);
}

#[test]
fn reachable_terminates_on_cycles() {
    let s = &[(1, 0), (5, 0)];
    let t = &[(2, 0), (6, 0)];
    let u = &[(1, 0), (7, 0)];
    let w = &[(2, 0), (8, 0)];
    let mut g = Graph::from_run(&run(vec![
        step(s, int(1)),
        step(t, int(2)),
        step(u, int(3)),
    ]));
    g.insert(&run(vec![
        step(t, int(2)),
        step(s, int(1)),
        step(w, int(4)),
    ]));
    let order = g.reachable();
    assert_eq!(order.len(), 4);
    assert_eq!(order[0], START);
    assert_eq!(g.edge_count(), 6);
    let pruned = g.pruned();
    assert_eq!(pruned.nodes().len(), 4);
    assert_eq!(pruned.key(), g.key());
}

#[test]
fn key_orders_by_edges_then_nodes_then_values() {
    let one = Graph::from_run(&left_run(false, 3));
    let mut two = one.clone();
    two.insert(&right_run(false, 5));
    assert!(one.key() < two.key());
    let coin = ChoiceValue::Boolean(false);
    let smaller = one.set_value(START, COIN, &coin, ChoiceValue::Boolean(false));
    assert_eq!(smaller.key(), one.key());
    let left = one.node(&at(&[(2, 0)])).unwrap();
    assert!(one.set_value(left, LEFT, &int(3), int(2)).key() < one.key());
    assert!(one.set_value(left, LEFT, &int(3), int(-2)).key() < one.key());
    assert!(
        one.set_value(left, LEFT, &int(3), ChoiceValue::Boolean(true))
            .key()
            < one.key()
    );
    let tied = two.set_value(START, COIN, &coin, ChoiceValue::Boolean(true));
    assert!(
        tied.nodes()[START]
            .edges
            .iter()
            .all(|e| e.value == ChoiceValue::Boolean(true))
    );
    assert_eq!(two.set_value(START, LEFT, &coin, int(1)).key(), two.key());
    let mut chain = Graph::from_run(&run(vec![step(COIN, int(1)), step(LEFT, int(1))]));
    chain.insert(&run(vec![step(COIN, int(1)), step(RIGHT, int(1))]));
    let mut flat = Graph::from_run(&run(vec![step(COIN, int(1)), step(LEFT, int(1))]));
    flat.insert(&run(vec![step(COIN, int(2)), step(LEFT, int(1))]));
    assert!(flat.key() < chain.key());
}

#[test]
fn pruned_drops_unreachable_nodes_and_keeps_end() {
    let mut g = Graph::from_run(&left_run(false, 3));
    g.insert(&right_run(true, 5));
    let cut = g.delete_edge(START, 1);
    assert_eq!(cut.nodes().len(), 4);
    let p = cut.pruned();
    assert_eq!(p.nodes().len(), 3);
    assert_eq!(p.nodes()[END].ident, Ident::End);
    assert_eq!(p.node(&at(&[(3, 0)])), None);
    assert_eq!(p.node(&at(&[(2, 0)])), Some(2));
    assert_eq!(p.nodes()[START].edges[0].target, 2);
    assert_eq!(p.nodes()[2].edges[0].target, END);
    assert_eq!(p.walk_verdict(&left_run(false, 3)), Walked::Whole);

    let left = g.node(&at(&[(2, 0)])).unwrap();
    let dead = g.delete_edge(START, 1).delete_edge(left, 0);
    assert_eq!(
        dead.nodes()[START].edges[0].target,
        END,
        "a node left without edges is where the run ends"
    );
    let p = dead.pruned();
    assert_eq!(p.nodes().len(), 2);
    assert_eq!(p.nodes()[START].ident, Ident::Start);
    assert_eq!(p.nodes()[END].ident, Ident::End);
    assert!(p.nodes()[END].edges.is_empty());
    assert_eq!(p.edge_count(), 1);
    assert_eq!(
        p.walk_verdict(&run(vec![step(COIN, ChoiceValue::Boolean(false))])),
        Walked::Whole
    );
}

#[test]
fn pruning_keeps_end_when_nothing_reaches_it() {
    let p = Graph::new().pruned();
    assert_eq!(p.nodes().len(), 2);
    assert_eq!(p.nodes()[END].ident, Ident::End);
    assert_eq!(p.edge_count(), 0);
}

#[test]
fn deleting_a_last_edge_merges_the_redirected_edge_with_an_existing_one() {
    let mut g = Graph::from_run(&left_run(false, 3));
    g.insert(&run(vec![step(COIN, ChoiceValue::Boolean(false))]));
    assert_eq!(
        g.nodes()[START].edges.len(),
        2,
        "a tie: on to LEFT, or the end"
    );
    let left = g.node(&at(&[(2, 0)])).unwrap();
    let cut = g.delete_edge(left, 0);
    assert_eq!(cut.nodes()[START].edges.len(), 1);
    assert_eq!(cut.nodes()[START].edges[0].target, END);
    assert_eq!(cut.pruned().edge_count(), 1);
}

#[test]
fn retain_settled_prunes_the_unsettled_alternatives_of_settled_ties() {
    let mut g = Graph::from_run(&left_run(false, 3));
    g.insert(&right_run(false, 5));
    g.insert(&left_run(false, 7));
    let right = g.node(&at(&[(3, 0)])).unwrap();
    let left = g.node(&at(&[(2, 0)])).unwrap();
    let mut untouched = g.clone();
    untouched.retain_settled(&[(left, 0)]);
    assert_eq!(untouched.edge_count(), g.edge_count());
    g.retain_settled(&[(START, 1), (right, 0)]);
    let start = &g.nodes()[START].edges;
    assert_eq!(start.len(), 1);
    assert_eq!(start[0].target, right);
    assert_eq!(g.nodes()[left].edges.len(), 2);
    assert_eq!(g.edge_count(), 2);
}

fn clone_value(children: Vec<ChoiceValue>) -> ChoiceValue {
    ChoiceValue::Clone(Arc::new(CloneRecord::from_values(children)))
}

#[test]
fn encode_decode_round_trip_covers_every_value_kind() {
    let realized = ChoiceValue::Clone(Arc::new(CloneRecord::from_stream(Arc::new(
        RealizedStream::new(
            vec![ChoiceNode::boolean(BooleanChoice { p: 0.5 }, true, false)],
            Vec::new(),
        ),
    ))));
    let mut g = Graph::from_run(&run(vec![
        step(COIN, ChoiceValue::Boolean(true)),
        step(LEFT, int(-12)),
        step(&[(2, 0), (12, 0)], ChoiceValue::Float(2.5)),
        step(&[(2, 0), (13, 0)], ChoiceValue::Bytes(vec![1, 2, 3])),
        step(&[(2, 0), (14, 0)], ChoiceValue::String(vec![104, 105])),
        step(
            &[(2, 0), (15, 0)],
            clone_value(vec![int(1), clone_value(vec![])]),
        ),
        step(&[(2, 0), (16, 0)], realized),
    ]));
    g.insert(&right_run(true, 5));
    let bytes = g.encode().unwrap();
    let back = Graph::decode(&bytes).unwrap();
    assert_eq!(back.nodes().len(), g.nodes().len());
    for (a, b) in back.nodes().iter().zip(g.nodes()) {
        assert_eq!(a.ident, b.ident);
        assert_eq!(a.edges, b.edges);
    }
    assert_eq!(back.node(&at(&[(3, 0)])), g.node(&at(&[(3, 0)])));
    assert_eq!(back.encode().unwrap(), bytes);
}

#[test]
fn encode_refuses_a_clone_nested_past_the_depth_limit() {
    let mut v = clone_value(Vec::new());
    for _ in 0..=MAX_CLONE_DEPTH {
        v = clone_value(vec![v]);
    }
    let g = Graph::from_run(&run(vec![step(COIN, v)]));
    assert!(g.encode().is_none());
}

struct Wire(Vec<u8>);

impl Wire {
    fn new(count: usize) -> Wire {
        let mut w = Wire(Vec::new());
        put_u32(&mut w.0, count);
        w
    }

    fn ident(mut self, ident: &Ident) -> Wire {
        match ident {
            Ident::Start => self.0.push(0),
            Ident::End => self.0.push(1),
            Ident::At(frames) => {
                self.0.push(2);
                put_frames(&mut self.0, frames);
            }
        }
        self
    }

    fn edges(mut self, count: usize) -> Wire {
        put_u32(&mut self.0, count);
        self
    }

    fn edge(mut self, addr: &[Frame], body: &[u8], target: usize) -> Wire {
        put_frames(&mut self.0, addr);
        put_u32(&mut self.0, body.len());
        self.0.extend_from_slice(body);
        put_u32(&mut self.0, target);
        self
    }

    fn raw(mut self, bytes: &[u8]) -> Wire {
        self.0.extend_from_slice(bytes);
        self
    }

    fn decode(&self) -> Option<Graph> {
        Graph::decode(&self.0)
    }
}

fn body(values: &[ChoiceValue]) -> Vec<u8> {
    serialize_choices(values).unwrap()
}

#[test]
fn decode_accepts_a_hand_built_graph() {
    let g = Wire::new(3)
        .ident(&Ident::Start)
        .edges(1)
        .edge(COIN, &body(&[int(1)]), 2)
        .ident(&Ident::End)
        .edges(0)
        .ident(&at(&[(2, 0)]))
        .edges(1)
        .edge(LEFT, &body(&[int(2)]), END)
        .decode()
        .unwrap();
    assert_eq!(
        g.walk_verdict(&run(vec![step(COIN, int(1)), step(LEFT, int(2))])),
        Walked::Whole
    );
}

#[test]
fn decode_rejects_malformed_input() {
    assert!(Graph::decode(&[]).is_none());
    assert!(Wire::new(1).decode().is_none());
    assert!(Wire::new(MAX_NODES + 1).decode().is_none());
    assert!(Wire::new(2).decode().is_none());
    assert!(Wire::new(2).raw(&[3]).decode().is_none());
    assert!(Wire::new(2).ident(&Ident::End).decode().is_none());
    assert!(
        Wire::new(2)
            .ident(&Ident::Start)
            .edges(0)
            .ident(&Ident::Start)
            .decode()
            .is_none()
    );
    assert!(
        Wire::new(2)
            .ident(&Ident::Start)
            .edges(0)
            .ident(&at(COIN))
            .decode()
            .is_none()
    );
    assert!(
        Wire::new(4)
            .ident(&Ident::Start)
            .edges(0)
            .ident(&Ident::End)
            .edges(0)
            .ident(&at(COIN))
            .edges(0)
            .ident(&at(COIN))
            .edges(0)
            .decode()
            .is_none()
    );
    assert!(Wire::new(2).ident(&Ident::Start).decode().is_none());
    assert!(
        Wire::new(2)
            .ident(&Ident::Start)
            .edges(MAX_EDGES + 1)
            .decode()
            .is_none()
    );
    let mut frames = Wire::new(2).ident(&Ident::Start).edges(1);
    put_u32(&mut frames.0, MAX_FRAMES + 1);
    assert!(frames.decode().is_none());
    let mut frames = Wire::new(2).ident(&Ident::Start).edges(1);
    put_u32(&mut frames.0, 1);
    assert!(frames.decode().is_none());
    assert!(frames.raw(&[0; 8]).decode().is_none());
    let mut short = Wire::new(2).ident(&Ident::Start).edges(1);
    put_frames(&mut short.0, COIN);
    put_u32(&mut short.0, 9);
    assert!(short.decode().is_none());
    assert!(
        Wire::new(2)
            .ident(&Ident::Start)
            .edges(1)
            .edge(COIN, &[9, 9, 9, 9, 9], END)
            .ident(&Ident::End)
            .edges(0)
            .decode()
            .is_none()
    );
    assert!(
        Wire::new(2)
            .ident(&Ident::Start)
            .edges(1)
            .edge(COIN, &body(&[int(1), int(2)]), END)
            .ident(&Ident::End)
            .edges(0)
            .decode()
            .is_none()
    );
    assert!(
        Wire::new(2)
            .ident(&Ident::Start)
            .edges(1)
            .edge(COIN, &body(&[int(1)]), 2)
            .ident(&Ident::End)
            .edges(0)
            .decode()
            .is_none()
    );
    assert!(
        Wire::new(2)
            .ident(&Ident::Start)
            .edges(0)
            .ident(&Ident::End)
            .edges(0)
            .raw(&[0])
            .decode()
            .is_none()
    );
}

#[test]
fn value_kinds_and_ranks() {
    let values = [
        ChoiceValue::Boolean(true),
        int(-3),
        ChoiceValue::Float(-2.5),
        ChoiceValue::Bytes(vec![0; 4]),
        ChoiceValue::String(vec![0; 5]),
        clone_value(vec![int(1), clone_value(vec![int(2), int(3)])]),
    ];
    let kinds: Vec<u8> = values.iter().map(value_kind).collect();
    assert_eq!(kinds, vec![0, 1, 2, 3, 4, 5]);
    let ranks: Vec<ValueRank> = values.iter().map(shrink_rank).collect();
    assert_eq!(
        ranks,
        vec![
            (0, 1, vec![]),
            (1, 3, vec![]),
            (2, float_to_index(2.5), vec![]),
            (3, 4, vec![0; 4]),
            (3, 5, vec![0; 5]),
            (4, 4, vec![])
        ]
    );
    assert!(shrink_rank(&ChoiceValue::Float(7.0)) < shrink_rank(&ChoiceValue::Float(7.5)));
    assert!(
        shrink_rank(&ChoiceValue::Bytes(vec![0, 4])) < shrink_rank(&ChoiceValue::Bytes(vec![3, 4]))
    );
    assert_eq!(shrink_rank(&ChoiceValue::Boolean(false)), (0, 0, vec![]));
    assert_eq!(
        shrink_rank(&ChoiceValue::Integer(BigInt::from(u128::MAX))),
        (1, u64::MAX, vec![])
    );
}
