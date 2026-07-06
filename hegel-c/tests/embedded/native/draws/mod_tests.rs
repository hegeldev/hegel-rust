use super::*;
use crate::native::core::{ManyState, NativeTestCase};
use crate::native::rng::EngineRng;

#[test]
fn many_reject_marks_invalid_when_cannot_reach_min_size() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(1));
    let mut state = ManyState::new(6, Some(10));
    state.count = 5;
    state.rejections = 9;

    let result = many_reject(&mut ntc, &mut state);
    assert!(
        result.is_err(),
        "expected StopTest once rejections overflow"
    );
    assert_eq!(ntc.status(), Some(Status::Invalid));
}

#[test]
fn many_more_respects_fixed_and_bounded_sizes() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(2));
    let mut fixed = ManyState::new(3, Some(3));
    let mut count = 0;
    while many_more(&mut ntc, &mut fixed).unwrap() {
        count += 1;
    }
    assert_eq!(count, 3);

    let mut bounded = ManyState::new(1, Some(4));
    let mut count = 0;
    while many_more(&mut ntc, &mut bounded).unwrap() {
        count += 1;
    }
    assert!((1..=4).contains(&count));
}
