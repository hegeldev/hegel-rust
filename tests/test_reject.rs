use hegel::generators as gs;
use hegel::{HealthCheck, TestCase};

#[hegel::test(suppress_health_check = HealthCheck::all())]
fn test_does_not_hang_on_reject(tc: TestCase) {
    tc.draw(gs::integers::<i32>());
    tc.reject();
}

#[hegel::test]
fn test_reject_filters_like_assume_false(tc: TestCase) {
    let n: i32 = tc.draw(gs::integers().min_value(0).max_value(100));
    if n >= 50 {
        tc.reject();
    }
    assert!(n < 50);
}

#[hegel::test]
fn test_reject_has_never_return_type(tc: TestCase) {
    let b: bool = tc.draw(gs::booleans());
    let n: i32 = if b { 1 } else { tc.reject() };
    assert_eq!(n, 1);
}
