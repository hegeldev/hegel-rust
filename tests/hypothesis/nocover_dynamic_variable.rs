//! Ported from hypothesis-python/tests/nocover/test_dynamic_variable.py
//!
//! Tests Hypothesis's internal `DynamicVariable` utility, ported to
//! `src/native/dynamic_variable.rs`. The Rust port uses a `Mutex<Vec<T>>`
//! stack rather than `threading.local()`, which preserves the scoped-
//! override / LIFO-nesting semantics the tests assert.

#![cfg(feature = "native")]

use hegel::__native_test_internals::DynamicVariable;

#[test]
fn test_can_assign() {
    let d = DynamicVariable::new(1);
    assert_eq!(d.value(), 1);
    d.with_value(2, || {
        assert_eq!(d.value(), 2);
    });
    assert_eq!(d.value(), 1);
}

#[test]
fn test_can_nest() {
    let d = DynamicVariable::new(1);
    d.with_value(2, || {
        assert_eq!(d.value(), 2);
        d.with_value(3, || {
            assert_eq!(d.value(), 3);
        });
        assert_eq!(d.value(), 2);
    });
    assert_eq!(d.value(), 1);
}
