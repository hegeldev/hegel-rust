use super::*;
use crate::native::core::{BUFFER_SIZE, MAX_CLONE_DEPTH};
use crate::native::rng::EngineRng;

fn draw(ntc: &mut NativeTestCase) -> i128 {
    ntc.draw_integer::<i128>(0, 1_000_000).unwrap()
}

#[test]
fn clone_stream_records_a_clone_node_and_hands_out_a_child() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(3));
    draw(&mut parent);
    let child = parent.clone_stream().unwrap();
    draw(&mut parent);
    draw(&mut child.lock().unwrap());

    assert_eq!(parent.nodes.len(), 3);
    assert_eq!(*parent.nodes[1].kind, ChoiceKind::Clone);
    assert!(!parent.nodes[1].was_forced);
    assert_eq!(child.lock().unwrap().nodes.len(), 1);
}

#[test]
fn clone_ids_count_per_stream_and_nest() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(3));
    assert!(parent.clone_id.is_empty());
    let first = parent.clone_stream().unwrap();
    let second = parent.clone_stream().unwrap();
    let nested = first.lock().unwrap().clone_stream().unwrap();
    assert_eq!(first.lock().unwrap().clone_id, vec![0]);
    assert_eq!(second.lock().unwrap().clone_id, vec![1]);
    assert_eq!(nested.lock().unwrap().clone_id, vec![0, 0]);
}

#[test]
fn clone_streams_are_deterministic_per_seed() {
    let run = |seed: u64| -> (Vec<i128>, Vec<i128>) {
        let mut parent = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut parent_vals = vec![draw(&mut parent)];
        let child = parent.clone_stream().unwrap();
        let mut child_guard = child.lock().unwrap();
        let child_vals = vec![draw(&mut child_guard), draw(&mut child_guard)];
        drop(child_guard);
        parent_vals.push(draw(&mut parent));
        (parent_vals, child_vals)
    };
    assert_eq!(run(99), run(99));
}

#[test]
fn child_draws_do_not_perturb_the_parents_stream() {
    let run = |child_draws: usize| -> Vec<i128> {
        let mut parent = NativeTestCase::new_random(EngineRng::seeded(21));
        let first = draw(&mut parent);
        let child = parent.clone_stream().unwrap();
        for _ in 0..child_draws {
            draw(&mut child.lock().unwrap());
        }
        vec![first, draw(&mut parent), draw(&mut parent)]
    };
    assert_eq!(run(0), run(5));
}

#[test]
fn reassemble_embeds_child_records_recursively() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(11));
    draw(&mut parent);
    let child = parent.clone_stream().unwrap();
    {
        let mut c = child.lock().unwrap();
        c.start_span(42);
        draw(&mut c);
        let grandchild = c.clone_stream().unwrap();
        draw(&mut grandchild.lock().unwrap());
        c.stop_span(false);
    }
    draw(&mut parent);
    parent.conclude(Status::Valid, None);
    parent.reassemble();

    let ChoiceValue::Clone(record) = &parent.nodes[1].value else {
        panic!("clone node was not realized");
    };
    let child_nodes = record.realized_nodes().unwrap();
    assert_eq!(child_nodes.len(), 2);
    assert_eq!(*child_nodes[1].kind, ChoiceKind::Clone);
    let ChoiceValue::Clone(inner) = &child_nodes[1].value else {
        panic!("nested clone node was not realized");
    };
    assert_eq!(inner.realized_nodes().unwrap().len(), 1);
    assert_eq!(record.spans().len(), 1);
    assert_eq!(record.spans()[0].label, "42");
    assert_eq!(record.spans()[0].start, 0);
    assert_eq!(record.spans()[0].end, 2);
    assert_eq!(record.span_events().len(), 2);
}

#[test]
fn replaying_a_reassembled_sequence_reproduces_every_stream() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(17));
    let p0 = draw(&mut parent);
    let child = parent.clone_stream().unwrap();
    let (c0, c1) = {
        let mut c = child.lock().unwrap();
        (draw(&mut c), draw(&mut c))
    };
    let p1 = draw(&mut parent);
    parent.conclude(Status::Valid, None);
    parent.reassemble();
    let choices: Vec<ChoiceValue> = parent.nodes.iter().map(|n| n.value.clone()).collect();

    let mut replay = NativeTestCase::for_choices(&choices, None, None);
    assert_eq!(draw(&mut replay), p0);
    let replay_child = replay.clone_stream().unwrap();
    {
        let mut c = replay_child.lock().unwrap();
        assert_eq!(draw(&mut c), c0);
        assert_eq!(draw(&mut c), c1);
    }
    assert_eq!(draw(&mut replay), p1);
    replay.conclude(Status::Valid, None);
    replay.reassemble();
    let replayed: Vec<ChoiceValue> = replay.nodes.iter().map(|n| n.value.clone()).collect();
    assert_eq!(replayed, choices);
}

#[test]
fn replay_child_overruns_when_it_draws_past_its_recorded_stream() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(29));
    let child = parent.clone_stream().unwrap();
    draw(&mut child.lock().unwrap());
    parent.conclude(Status::Valid, None);
    parent.reassemble();
    let choices: Vec<ChoiceValue> = parent.nodes.iter().map(|n| n.value.clone()).collect();

    let mut replay = NativeTestCase::for_choices(&choices, None, None);
    let replay_child = replay.clone_stream().unwrap();
    let mut c = replay_child.lock().unwrap();
    draw(&mut c);
    assert!(matches!(
        c.draw_integer::<i128>(0, 10),
        Err(EngineError::Overrun)
    ));
    assert_eq!(replay.status(), Some(Status::EarlyStop));
}

#[test]
fn clone_at_a_non_clone_prefix_position_puns_to_an_empty_child() {
    let mut replay =
        NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(5))], None, None);
    let child = replay.clone_stream().unwrap();
    let result = child.lock().unwrap().draw_integer::<i128>(0, 10);
    assert!(matches!(result, Err(EngineError::Overrun)));
    assert_eq!(replay.status(), Some(Status::EarlyStop));
}

#[test]
fn probe_replay_extends_a_punned_child_randomly() {
    let mut replay = NativeTestCase::for_probe(
        &[ChoiceValue::Integer(BigInt::from(5))],
        EngineRng::seeded(1),
        BUFFER_SIZE,
    );
    let child = replay.clone_stream().unwrap();
    draw(&mut child.lock().unwrap());
    assert_eq!(replay.status(), None);
}

#[test]
fn family_conclusion_stops_every_stream() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(5));
    let child = parent.clone_stream().unwrap();
    child.lock().unwrap().conclude(Status::Invalid, None);
    assert!(matches!(
        parent.draw_integer::<i128>(0, 10),
        Err(EngineError::InvalidTestCase)
    ));
    assert_eq!(parent.status(), Some(Status::Invalid));
    assert_eq!(child.lock().unwrap().status(), Some(Status::Invalid));
}

#[test]
fn conclusion_is_write_once_across_streams() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(5));
    let child = parent.clone_stream().unwrap();
    parent.conclude(
        Status::Interesting,
        Some(InterestingOrigin("first".to_string())),
    );
    child.lock().unwrap().conclude(Status::Valid, None);
    assert_eq!(parent.status(), Some(Status::Interesting));
    let (_, origin) = parent.family().conclusion().unwrap();
    assert_eq!(origin, Some(InterestingOrigin("first".to_string())));
}

#[test]
fn family_budget_caps_total_draws_across_streams() {
    let mut parent = NativeTestCase::for_probe(&[], EngineRng::seeded(13), 4);
    draw(&mut parent);
    draw(&mut parent);
    let child = parent.clone_stream().unwrap();
    let mut c = child.lock().unwrap();
    draw(&mut c);
    assert!(matches!(
        c.draw_integer::<i128>(0, 10),
        Err(EngineError::Overrun)
    ));
    assert_eq!(parent.status(), Some(Status::EarlyStop));
}

#[test]
fn clone_nesting_beyond_max_depth_is_invalid() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(7));
    let mut handles = Vec::new();
    let mut current = parent.clone_stream().unwrap();
    for _ in 1..MAX_CLONE_DEPTH {
        let next = current.lock().unwrap().clone_stream().unwrap();
        handles.push(current);
        current = next;
    }
    let too_deep = current.lock().unwrap().clone_stream();
    assert!(matches!(too_deep, Err(EngineError::InvalidTestCase)));
    assert_eq!(parent.status(), Some(Status::Invalid));
}

#[test]
fn clone_stream_fails_after_the_family_has_concluded() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(7));
    parent.conclude(Status::Valid, None);
    assert!(matches!(parent.clone_stream(), Err(EngineError::Overrun)));
}

#[test]
fn clone_node_consumes_a_slot_in_the_stream_budget() {
    let mut parent = NativeTestCase::for_probe(&[], EngineRng::seeded(13), 1);
    parent.clone_stream().unwrap();
    assert!(matches!(
        parent.draw_integer::<i128>(0, 10),
        Err(EngineError::Overrun)
    ));
}

#[test]
fn simplest_template_children_resolve_to_simplest_values() {
    let mut parent = NativeTestCase::for_simplest(BUFFER_SIZE);
    assert_eq!(draw(&mut parent), 0);
    let child = parent.clone_stream().unwrap();
    assert_eq!(draw(&mut child.lock().unwrap()), 0);
    assert_eq!(draw(&mut parent), 0);
}

#[test]
fn reassembled_values_flow_through_probe_prefixes() {
    let mut parent = NativeTestCase::new_random(EngineRng::seeded(41));
    let p0 = draw(&mut parent);
    let child = parent.clone_stream().unwrap();
    let c0 = draw(&mut child.lock().unwrap());
    parent.conclude(Status::Valid, None);
    parent.reassemble();
    let choices: Vec<ChoiceValue> = parent.nodes.iter().map(|n| n.value.clone()).collect();

    let mut probe = NativeTestCase::for_probe(&choices, EngineRng::seeded(2), BUFFER_SIZE);
    assert_eq!(draw(&mut probe), p0);
    let probe_child = probe.clone_stream().unwrap();
    {
        let mut c = probe_child.lock().unwrap();
        assert_eq!(draw(&mut c), c0);
        draw(&mut c);
    }
    draw(&mut probe);
    assert_eq!(probe.status(), None);
}

#[test]
fn concurrent_draws_on_separate_streams_are_deterministic() {
    let run = || -> (Vec<i128>, Vec<i128>) {
        let mut parent = NativeTestCase::new_random(EngineRng::seeded(1234));
        let child = parent.clone_stream().unwrap();
        let worker = std::thread::spawn(move || {
            let mut vals = Vec::new();
            for _ in 0..100 {
                vals.push(draw(&mut child.lock().unwrap()));
            }
            vals
        });
        let mut parent_vals = Vec::new();
        for _ in 0..100 {
            parent_vals.push(draw(&mut parent));
        }
        let child_vals = worker.join().unwrap();
        (parent_vals, child_vals)
    };
    let (p1, c1) = run();
    let (p2, c2) = run();
    assert_eq!(p1, p2);
    assert_eq!(c1, c2);
}
