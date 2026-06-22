use super::*;
use crate::native::rng::EngineRng;

#[test]
fn cbor_to_f64_from_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer(42.into())).unwrap(), 42.0);
}

#[test]
fn cbor_to_f64_from_negative_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer((-7i64).into())).unwrap(), -7.0);
}

#[test]
fn cbor_to_f64_non_numeric_is_invalid_argument() {
    let err = cbor_to_f64(&Value::Bool(true)).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("CBOR float or integer"));
}

// interpret_float must reject widths outside `{32, 64}`: Hypothesis only
// supports `{16, 32, 64}` and we have no Rust `f16` to back width 16, so
// the schema interpreter fails loud at the boundary rather than silently
// treating unknown widths as f64.

#[test]
fn interpret_float_rejects_width_16() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema = cbor_map! { "type" => "float", "width" => 16 };
    let err = interpret_float(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("unsupported float width"));
}

#[test]
fn interpret_float_rejects_width_128() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema = cbor_map! { "type" => "float", "width" => 128 };
    let err = interpret_float(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("unsupported float width"));
}

#[test]
fn interpret_float_non_numeric_bound_is_invalid_argument() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema = cbor_map! { "type" => "float", "min_value" => "low" };
    let err = interpret_float(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("CBOR float or integer"));
}

// `min_value=-inf, exclude_min=true` is the documented Hypothesis idiom for
// "any float except -inf": `next_up(-inf)` is `-f64::MAX`. The interpreter
// used to skip the adjustment for non-finite bounds, silently keeping -inf
// generable.

#[test]
fn interpret_float_exclude_min_excludes_negative_infinity() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let schema = cbor_map! {
        "type" => "float",
        "min_value" => f64::NEG_INFINITY,
        "exclude_min" => true,
        "allow_nan" => false
    };
    for seed in 0..200 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let v = interpret_float(&mut ntc, &schema).unwrap();
        let f = v.as_float().unwrap();
        assert_ne!(f, f64::NEG_INFINITY, "drew -inf despite exclude_min");
    }
}

#[test]
fn interpret_float_exclude_max_excludes_positive_infinity() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let schema = cbor_map! {
        "type" => "float",
        "max_value" => f64::INFINITY,
        "exclude_max" => true,
        "allow_nan" => false
    };
    for seed in 0..200 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let v = interpret_float(&mut ntc, &schema).unwrap();
        let f = v.as_float().unwrap();
        assert_ne!(f, f64::INFINITY, "drew +inf despite exclude_max");
    }
}

#[test]
fn interpret_float_respects_smallest_nonzero_magnitude() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let schema = cbor_map! {
        "type" => "float",
        "min_value" => -1.0,
        "max_value" => 1.0,
        "allow_nan" => false,
        "allow_infinity" => false,
        "smallest_nonzero_magnitude" => f64::MIN_POSITIVE
    };
    for seed in 0..200 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let v = interpret_float(&mut ntc, &schema).unwrap();
        let f = v.as_float().unwrap();
        assert!(
            f == 0.0 || f.abs() >= f64::MIN_POSITIVE,
            "drew {f}, inside the excluded magnitude band"
        );
    }
}

#[test]
fn interpret_float_rejects_non_positive_smallest_nonzero_magnitude() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    for bad in [0.0, -1.0, f64::NAN] {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
        let schema = cbor_map! {
            "type" => "float",
            "smallest_nonzero_magnitude" => bad
        };
        let err = interpret_float(&mut ntc, &schema).unwrap_err();
        assert!(matches!(err, EngineError::InvalidArgument(_)));
    }
}

#[test]
fn interpret_float_accepts_width_32_and_64() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema_64 = cbor_map! { "type" => "float", "width" => 64 };
    assert!(interpret_float(&mut ntc, &schema_64).is_ok());

    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let schema_32 = cbor_map! { "type" => "float", "width" => 32 };
    assert!(interpret_float(&mut ntc, &schema_32).is_ok());
}
