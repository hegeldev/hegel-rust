use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn raise_internal_error_panics_directly_outside_a_test_context() {
    // Outside a run there is no lifecycle to catch a control payload, so
    // the message (with location and bug-report framing) panics directly.
    let payload = catch_unwind(|| raise_internal_error(format_args!("boom: {}", 7))).unwrap_err();
    let msg = panic_message(payload);
    assert!(msg.contains("Internal error in hegel at "), "{msg}");
    assert!(msg.contains("boom: 7"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn raise_internal_error_unwinds_as_a_typed_payload_inside_a_test_context() {
    let payload = with_test_context(|| {
        catch_unwind(|| raise_internal_error(format_args!("inner"))).unwrap_err()
    });
    let internal = payload
        .downcast_ref::<InternalError>()
        .expect("expected an InternalError control payload");
    assert!(internal.0.contains("inner"), "{}", internal.0);
}

#[test]
fn raise_invalid_argument_panics_directly_outside_a_test_context() {
    let payload =
        catch_unwind(|| raise_invalid_argument(format_args!("bad arg: {}", 1))).unwrap_err();
    let msg = panic_message(payload);
    assert!(msg.contains("bad arg: 1"), "{msg}");
}

#[test]
fn raise_invalid_argument_unwinds_as_a_typed_payload_inside_a_test_context() {
    let payload = with_test_context(|| {
        catch_unwind(|| raise_invalid_argument(format_args!("bad arg"))).unwrap_err()
    });
    let ia = payload
        .downcast_ref::<InvalidArgument>()
        .expect("expected an InvalidArgument control payload");
    assert!(ia.0.contains("bad arg"), "{}", ia.0);
}

#[test]
fn internal_assert_includes_the_condition_when_it_fails() {
    let value = 3;
    let payload = catch_unwind(|| hegel_internal_assert!(value == 4)).unwrap_err();
    let msg = panic_message(payload);
    assert!(
        msg.contains("internal assertion failed: value == 4"),
        "{msg}"
    );
}

#[test]
fn internal_assert_passes_silently() {
    hegel_internal_assert!(1 + 1 == 2);
    hegel_internal_assert!(1 + 1 == 2, "with a message {}", "argument");
}

#[test]
fn internal_assert_eq_reports_both_values() {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        hegel_internal_assert_eq!(2 + 2, 5);
    }))
    .unwrap_err();
    let msg = panic_message(payload);
    assert!(msg.contains("2 + 2 == 5"), "{msg}");
    assert!(msg.contains("left: 4, right: 5"), "{msg}");
    hegel_internal_assert_eq!(2 + 2, 4);
}

#[test]
fn internal_assert_ne_reports_the_shared_value() {
    let payload = catch_unwind(AssertUnwindSafe(|| {
        hegel_internal_assert_ne!(2 + 2, 4);
    }))
    .unwrap_err();
    let msg = panic_message(payload);
    assert!(msg.contains("2 + 2 != 4"), "{msg}");
    assert!(msg.contains("both: 4"), "{msg}");
    hegel_internal_assert_ne!(2 + 2, 5);
}

#[test]
fn internal_debug_asserts_follow_debug_assertions() {
    let fired = catch_unwind(AssertUnwindSafe(|| {
        hegel_internal_debug_assert!(false);
    }))
    .is_err();
    assert_eq!(fired, cfg!(debug_assertions));

    let fired = catch_unwind(AssertUnwindSafe(|| {
        hegel_internal_debug_assert_eq!(1, 2);
    }))
    .is_err();
    assert_eq!(fired, cfg!(debug_assertions));

    let fired = catch_unwind(AssertUnwindSafe(|| {
        hegel_internal_debug_assert_ne!(1, 1);
    }))
    .is_err();
    assert_eq!(fired, cfg!(debug_assertions));

    // The passing direction is free either way.
    hegel_internal_debug_assert!(true);
    hegel_internal_debug_assert_eq!(1, 1);
    hegel_internal_debug_assert_ne!(1, 2);
}
