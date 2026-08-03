use super::*;

fn assert_err<T: std::fmt::Debug>(result: Result<T, InternalError>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn internal_error_carries_location_and_bug_report_framing() {
    let e = InternalError::new(format_args!("boom: {}", 7));
    let msg = e.to_string();
    assert!(msg.contains("Internal error in hegel at "), "{msg}");
    assert!(msg.contains(file!()), "{msg}");
    assert!(msg.contains("boom: 7"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
    assert!(
        msg.contains("https://github.com/hegeldev/hegel-rust/issues"),
        "{msg}"
    );
}

#[test]
fn internal_error_is_clonable_and_compares_equal_to_itself() {
    let e = InternalError::new(format_args!("boom"));
    assert_eq!(e.clone(), e);
    assert!(format!("{e:?}").contains("boom"));
}

#[test]
fn hegel_internal_error_returns_err_with_the_message() {
    fn raise() -> Result<(), InternalError> {
        hegel_internal_error!("kaboom {}", 3);
    }
    let msg = assert_err(raise());
    assert!(msg.contains("kaboom 3"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn internal_assert_includes_the_condition_when_it_fails() {
    fn check(value: i32) -> Result<(), InternalError> {
        hegel_internal_assert!(value == 4);
        Ok(())
    }
    let msg = assert_err(check(3));
    assert!(
        msg.contains("internal assertion failed: value == 4"),
        "{msg}"
    );
}

#[test]
fn internal_assert_passes_silently() {
    fn check() -> Result<(), InternalError> {
        hegel_internal_assert!(1 + 1 == 2);
        hegel_internal_assert!(1 + 1 == 2, "with a message {}", "argument");
        Ok(())
    }
    check().unwrap();
}

#[test]
fn internal_assert_eq_reports_both_values() {
    fn check(a: i32, b: i32) -> Result<(), InternalError> {
        hegel_internal_assert_eq!(a + 2, b);
        Ok(())
    }
    let msg = assert_err(check(2, 5));
    assert!(msg.contains("a + 2 == b"), "{msg}");
    assert!(msg.contains("left: 4, right: 5"), "{msg}");
    check(2, 4).unwrap();
}

#[test]
fn internal_assert_ne_reports_the_shared_value() {
    fn check(a: i32, b: i32) -> Result<(), InternalError> {
        hegel_internal_assert_ne!(a + 2, b);
        Ok(())
    }
    let msg = assert_err(check(2, 4));
    assert!(msg.contains("a + 2 != b"), "{msg}");
    assert!(msg.contains("both: 4"), "{msg}");
    check(2, 5).unwrap();
}

#[test]
fn internal_debug_asserts_follow_debug_assertions() {
    fn check_assert() -> Result<(), InternalError> {
        hegel_internal_debug_assert!(false);
        Ok(())
    }
    assert_eq!(check_assert().is_err(), cfg!(debug_assertions));

    fn check_eq() -> Result<(), InternalError> {
        hegel_internal_debug_assert_eq!(1, 2);
        Ok(())
    }
    assert_eq!(check_eq().is_err(), cfg!(debug_assertions));

    fn check_ne() -> Result<(), InternalError> {
        hegel_internal_debug_assert_ne!(1, 1);
        Ok(())
    }
    assert_eq!(check_ne().is_err(), cfg!(debug_assertions));

    fn check_passing() -> Result<(), InternalError> {
        hegel_internal_debug_assert!(true);
        hegel_internal_debug_assert_eq!(1, 1);
        hegel_internal_debug_assert_ne!(1, 2);
        Ok(())
    }
    check_passing().unwrap();
}
