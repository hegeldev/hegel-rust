use super::*;
use crate::native::core::{ManyState, NativeTestCase};
use crate::native::rng::EngineRng;

fn float_spec(width: u32, min_value: f64, max_value: f64) -> FloatSpec {
    FloatSpec {
        width,
        min_value,
        max_value,
        allow_nan: false,
        allow_infinity: false,
        exclude_min: false,
        exclude_max: false,
        smallest_nonzero_magnitude: f64::MIN_POSITIVE,
    }
}

#[test]
fn width_32_float_bounds_must_be_f32_representable() {
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(3));

    for (min_value, max_value, bad) in [
        (1.0f64.next_up(), 2.0, "min_value"),
        (0.0, 1.0f64.next_up(), "max_value"),
        (f64::MAX, f64::MAX, "min_value"),
    ] {
        let err = generate_float(&mut ntc, &float_spec(32, min_value, max_value)).unwrap_err();
        let EngineError::InvalidArgument(msg) = err else {
            panic!("expected InvalidArgument, got {err:?}");
        };
        assert!(
            msg.contains(bad) && msg.contains("width 32"),
            "unexpected message: {msg}"
        );
    }

    let v = generate_float(&mut ntc, &float_spec(32, 0.5, 2.0)).unwrap();
    assert!((0.5..=2.0).contains(&v));

    let mut inf_spec = float_spec(32, f64::NEG_INFINITY, f64::INFINITY);
    inf_spec.allow_infinity = true;
    generate_float(&mut ntc, &inf_spec).unwrap();

    let v = generate_float(&mut ntc, &float_spec(64, 1.0f64.next_up(), 2.0)).unwrap();
    assert!((1.0f64.next_up()..=2.0).contains(&v));
}

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
