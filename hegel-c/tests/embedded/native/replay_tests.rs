//! Embedded tests for `src/native/core/replay.rs`: the live-set replay
//! semantics, driven through `NativeTestCase`.

use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::{CloneRecord, NativeTestCase, Status};
use crate::native::graph::DRAW_LABEL;
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

fn replay(timelines: Vec<Vec<ChoiceValue>>) -> NativeTestCase {
    NativeTestCase::for_counterexample(&timelines, EngineRng::seeded(5), 64).unwrap()
}

fn draw_int(tc: &mut NativeTestCase) -> i64 {
    tc.draw_integer::<i64>(0, 1000).unwrap()
}

fn draw_bool(tc: &mut NativeTestCase) -> bool {
    tc.weighted(0.5, None).unwrap()
}

#[test]
fn a_branch_visible_at_the_divergence_is_followed_without_diverging() {
    let mut tc = replay(vec![
        vec![boolean(true), int(5)],
        vec![boolean(true), boolean(false), int(7)],
    ]);
    assert!(draw_bool(&mut tc));
    assert_eq!(tc.live_timelines(), vec![true, true]);
    assert!(!draw_bool(&mut tc));
    assert_eq!(tc.live_timelines(), vec![false, true]);
    assert_eq!(draw_int(&mut tc), 7);
    assert_eq!(tc.divergence(), None);
}

#[test]
fn the_first_live_timeline_serves_and_disagreeing_ones_leave() {
    let mut tc = replay(vec![
        vec![boolean(true), int(1)],
        vec![boolean(true), int(2)],
    ]);
    draw_bool(&mut tc);
    assert_eq!(draw_int(&mut tc), 1);
    assert_eq!(tc.live_timelines(), vec![true, false]);
    assert_eq!(tc.divergence(), None);
}

#[test]
fn running_off_every_timeline_diverges_and_draws_randomly() {
    let mut tc = replay(vec![
        vec![boolean(true), int(1)],
        vec![boolean(true), int(2)],
    ]);
    draw_bool(&mut tc);
    draw_int(&mut tc);
    draw_int(&mut tc);
    assert_eq!(tc.live_timelines(), vec![false, false]);
    assert_eq!(
        tc.divergence(),
        Some(Divergence {
            stream: vec![],
            position: 2
        })
    );
    assert_eq!(tc.nodes.len(), 3);
}

#[test]
fn a_divergence_visible_later_continues_from_the_pruned_timeline_that_fits() {
    let mut tc = replay(vec![
        vec![boolean(true), int(1), int(3)],
        vec![boolean(true), int(2), boolean(false), int(9)],
    ]);
    draw_bool(&mut tc);
    assert_eq!(draw_int(&mut tc), 1);
    assert!(!draw_bool(&mut tc));
    assert_eq!(
        tc.divergence(),
        Some(Divergence {
            stream: vec![],
            position: 2
        })
    );
    assert_eq!(draw_int(&mut tc), 9);
}

#[test]
fn the_most_recently_pruned_timeline_is_the_first_donor() {
    let mut tc = replay(vec![
        vec![int(1), int(2), boolean(true)],
        vec![int(1), int(3), int(30)],
        vec![int(5), int(4), int(40)],
    ]);
    draw_int(&mut tc);
    assert_eq!(tc.live_timelines(), vec![true, true, false]);
    draw_int(&mut tc);
    assert_eq!(tc.live_timelines(), vec![true, false, false]);
    draw_int(&mut tc);
    assert_eq!(tc.live_timelines(), vec![false, false, false]);
    assert_eq!(tc.nodes[2].value(), int(30));
}

#[test]
fn a_divergence_is_recorded_once() {
    let mut tc = replay(vec![vec![boolean(true)]]);
    draw_bool(&mut tc);
    draw_int(&mut tc);
    draw_int(&mut tc);
    assert_eq!(
        tc.divergence(),
        Some(Divergence {
            stream: vec![],
            position: 1
        })
    );
}

#[test]
fn a_cloned_stream_prunes_for_its_parent() {
    let mut tc = replay(vec![
        vec![clone_of(vec![int(1), int(2)]), int(10)],
        vec![clone_of(vec![int(1), boolean(true)]), int(20)],
    ]);
    let child = tc.clone_stream().unwrap();
    assert_eq!(draw_int(&mut child.lock()), 1);
    assert_eq!(tc.live_timelines(), vec![true, true]);
    assert!(draw_bool(&mut child.lock()));
    assert_eq!(tc.live_timelines(), vec![false, true]);
    assert_eq!(draw_int(&mut tc), 20);
    assert_eq!(tc.divergence(), None);
    draw_int(&mut child.lock());
    assert_eq!(
        tc.divergence(),
        Some(Divergence {
            stream: vec![0],
            position: 2
        })
    );
}

#[test]
fn a_timeline_without_a_clone_at_a_clone_position_leaves_the_live_set() {
    let mut tc = replay(vec![vec![int(1)], vec![clone_of(vec![int(2)])]]);
    let child = tc.clone_stream().unwrap();
    assert_eq!(tc.live_timelines(), vec![false, true]);
    assert_eq!(draw_int(&mut child.lock()), 2);
    assert_eq!(tc.divergence(), None);
}

#[test]
fn a_clone_position_no_live_timeline_has_diverges_and_donors_still_serve_the_child() {
    let mut tc = replay(vec![
        vec![int(1), int(2)],
        vec![int(3), clone_of(vec![int(7)])],
    ]);
    draw_int(&mut tc);
    assert_eq!(tc.live_timelines(), vec![true, false]);
    let child = tc.clone_stream().unwrap();
    assert_eq!(
        tc.divergence(),
        Some(Divergence {
            stream: vec![],
            position: 1
        })
    );
    assert_eq!(draw_int(&mut child.lock()), 7);
}

#[test]
fn the_pun_replay_puns_a_misfit_and_never_diverges() {
    let mut tc = NativeTestCase::for_choices(&[int(5), boolean(true)], None, None);
    draw_bool(&mut tc);
    assert!(matches!(tc.nodes[0].value(), ChoiceValue::Boolean(_)));
    assert!(draw_bool(&mut tc));
    assert_eq!(tc.divergence(), None);
    assert_eq!(tc.live_timelines(), vec![true]);
}

#[test]
fn the_pun_replay_hands_an_empty_child_for_a_non_clone_at_a_clone_position() {
    let mut tc = NativeTestCase::for_probe(&[int(5)], EngineRng::seeded(1), 8).unwrap();
    let child = tc.clone_stream().unwrap();
    draw_int(&mut child.lock());
    assert_eq!(child.lock().nodes.len(), 1);
    assert_eq!(tc.divergence(), None);
}

#[test]
fn the_pun_replay_past_its_prefix_draws_randomly() {
    let mut tc = NativeTestCase::for_probe(&[], EngineRng::seeded(1), 8).unwrap();
    let child = tc.clone_stream().unwrap();
    draw_int(&mut child.lock());
    draw_int(&mut tc);
    assert_eq!(tc.nodes.len(), 2);
    assert_eq!(tc.divergence(), None);
}

#[test]
fn the_pun_replay_hands_a_child_its_realized_nodes_for_the_simplest_pun() {
    let mut parent = NativeTestCase::for_probe(&[], EngineRng::seeded(9), 16).unwrap();
    let child = parent.clone_stream().unwrap();
    child.lock().draw_integer::<i64>(0, 0).unwrap();
    parent.conclude(Status::Valid, None);
    parent.reassemble();
    let nodes = parent.nodes.clone();
    let mut tc = NativeTestCase::for_choices(
        &nodes.iter().map(|n| n.value()).collect::<Vec<_>>(),
        Some(&nodes),
        None,
    );
    let child = tc.clone_stream().unwrap();
    assert_eq!(
        child.lock().draw_integer::<i64>(3, 4).unwrap(),
        3,
        "the stored 0 was its node's simplest, so the misfit puns to the new simplest"
    );
}

#[test]
fn an_empty_counterexample_generates_freshly_and_has_nothing_to_leave() {
    let mut tc = replay(vec![]);
    draw_int(&mut tc);
    assert_eq!(tc.live_timelines(), Vec::<bool>::new());
    assert_eq!(tc.nodes.len(), 1);
    assert_eq!(tc.divergence(), None);
}

#[test]
fn a_pool_member_serves_the_branch_the_incumbent_cannot() {
    let mut tc = replay(vec![
        vec![boolean(true), boolean(true), int(68)],
        vec![boolean(true), int(89), int(68)],
        vec![boolean(true), int(64), int(68)],
    ]);
    assert!(draw_bool(&mut tc));
    assert_eq!(tc.live_timelines(), vec![true, true, true]);
    assert_eq!(draw_int(&mut tc), 89);
    assert_eq!(tc.live_timelines(), vec![false, true, false]);
    assert_eq!(draw_int(&mut tc), 68);
    assert_eq!(tc.divergence(), None);
}

type Frames = Arc<Mutex<Vec<Vec<(u64, usize)>>>>;

struct Scripted {
    values: Vec<ChoiceValue>,
    seen: Vec<(Vec<usize>, usize)>,
    frames: Frames,
    divergence: Option<Divergence>,
}

impl ExternalReplay for Scripted {
    fn resolve(
        &mut self,
        stream: &[usize],
        position: usize,
        frames: &[(u64, usize)],
        fits: &dyn Fn(&ChoiceValue) -> bool,
    ) -> Option<ChoiceValue> {
        self.seen.push((stream.to_vec(), position));
        self.frames.lock().push(frames.to_vec());
        let stored = self.values.get(self.seen.len() - 1)?.clone();
        if !fits(&stored) && self.divergence.is_none() {
            self.divergence = Some(Divergence {
                stream: stream.to_vec(),
                position,
            });
        }
        Some(stored)
    }

    fn divergence(&self) -> Option<Divergence> {
        self.divergence.clone()
    }

    fn longest(&self) -> usize {
        self.values.len()
    }
}

fn scripted_with_frames(values: Vec<ChoiceValue>) -> (NativeTestCase, Frames) {
    let frames = Arc::new(Mutex::new(Vec::new()));
    let resolver = Scripted {
        values,
        seen: Vec::new(),
        frames: Arc::clone(&frames),
        divergence: None,
    };
    let tc = NativeTestCase::for_external(Box::new(resolver), EngineRng::seeded(5), 8).unwrap();
    (tc, frames)
}

fn scripted(values: Vec<ChoiceValue>) -> NativeTestCase {
    scripted_with_frames(values).0
}

#[test]
fn an_external_resolver_is_told_the_open_spans_and_their_sibling_ordinals() {
    let (mut tc, frames) = scripted_with_frames(vec![int(1), int(2), int(3), int(4)]);
    tc.start_span(7);
    draw_int(&mut tc);
    tc.stop_span(false);
    tc.start_span(7);
    tc.start_span(9);
    draw_int(&mut tc);
    tc.stop_span(false);
    tc.start_span(9);
    draw_int(&mut tc);
    draw_int(&mut tc);
    assert_eq!(
        *frames.lock(),
        vec![
            vec![(7, 0), (DRAW_LABEL, 0)],
            vec![(7, 1), (9, 0), (DRAW_LABEL, 0)],
            vec![(7, 1), (9, 1), (DRAW_LABEL, 0)],
            vec![(7, 1), (9, 1), (DRAW_LABEL, 1)],
        ]
    );
}

#[test]
fn an_external_resolver_serves_fitting_values_and_the_run_draws_past_it() {
    let mut tc = scripted(vec![int(7), boolean(true)]);
    assert_eq!(draw_int(&mut tc), 7);
    assert!(draw_bool(&mut tc));
    draw_int(&mut tc);
    assert_eq!(tc.nodes.len(), 3);
    assert_eq!(tc.divergence(), None);
    assert_eq!(tc.live_timelines(), Vec::<bool>::new());
}

#[test]
fn an_external_resolver_reports_its_own_divergence_and_a_misfit_draws_randomly() {
    let mut tc = scripted(vec![boolean(true), int(3)]);
    draw_int(&mut tc);
    assert_eq!(
        tc.divergence(),
        Some(Divergence {
            stream: vec![],
            position: 0
        })
    );
    assert_eq!(draw_int(&mut tc), 3);
}

#[test]
fn a_cloned_stream_resolves_through_the_same_external_resolver() {
    let mut tc = scripted(vec![int(1), int(2)]);
    assert_eq!(draw_int(&mut tc), 1);
    let child = tc.clone_stream().unwrap();
    assert_eq!(draw_int(&mut child.lock()), 2);
    assert_eq!(tc.divergence(), None);
}
