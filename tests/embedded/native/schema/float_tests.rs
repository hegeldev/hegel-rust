use super::*;

#[test]
fn cbor_to_f64_from_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer(42.into())), 42.0);
}

#[test]
fn cbor_to_f64_from_negative_integer() {
    assert_eq!(cbor_to_f64(&Value::Integer((-7i64).into())), -7.0);
}

#[test]
#[should_panic(expected = "Expected CBOR float/integer")]
fn cbor_to_f64_panics_on_non_numeric() {
    let _ = cbor_to_f64(&Value::Bool(true));
}

// ── B3: interpret_float rejects unsupported widths ───────────────────────
//
// Pre-B3 only `width == 32` was special-cased; widths in `{16, 128, ...}`
// silently fell through and were treated as f64.  Hypothesis only
// supports widths `{16, 32, 64}` (and we don't have an `f16` Rust type
// to back `width=16`), so anything other than 32 or 64 should fail loud
// at the boundary rather than silently round-trip through f64.

#[test]
#[should_panic(expected = "unsupported float width")]
fn interpret_float_rejects_width_16() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    let schema = cbor_map! { "type" => "float", "width" => 16 };
    let _ = interpret_float(&mut ntc, &schema);
}

#[test]
#[should_panic(expected = "unsupported float width")]
fn interpret_float_rejects_width_128() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    let schema = cbor_map! { "type" => "float", "width" => 128 };
    let _ = interpret_float(&mut ntc, &schema);
}

#[test]
fn interpret_float_accepts_width_32_and_64() {
    use crate::cbor_utils::cbor_map;
    use crate::native::core::NativeTestCase;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    let schema_64 = cbor_map! { "type" => "float", "width" => 64 };
    assert!(interpret_float(&mut ntc, &schema_64).is_ok());

    let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(0));
    let schema_32 = cbor_map! { "type" => "float", "width" => 32 };
    assert!(interpret_float(&mut ntc, &schema_32).is_ok());
}
