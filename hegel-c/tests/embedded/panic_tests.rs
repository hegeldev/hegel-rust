use super::*;

#[test]
fn extracts_str_payload() {
    let payload: Box<dyn std::any::Any + Send> = Box::new("boom");
    assert_eq!(panic_message(&payload), "boom");
}

#[test]
fn extracts_string_payload() {
    let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("kaboom"));
    assert_eq!(panic_message(&payload), "kaboom");
}

#[test]
fn falls_back_for_an_unknown_payload_type() {
    // A payload that is neither `&str` nor `String` (the two shapes
    // `std::panic` produces) yields the generic fallback.
    let payload: Box<dyn std::any::Any + Send> = Box::new(42i32);
    assert_eq!(panic_message(&payload), "Unknown panic");
}
