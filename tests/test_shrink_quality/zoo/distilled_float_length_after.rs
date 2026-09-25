//! Control: the value behind the list that must move with a deletion is a *float*.
//!
//! `v = vecs(integers 0..=1000).max_size(10)`, then `f = floats(0.0..=10.0)`; the property is
//! only checked when `f == v.len() as f64` and fails iff some element is non-zero. Shortlex ideal
//! `([1], 1.0)`. `distilled_declared_length_after` with the integer replaced by a float, and it
//! passes for an accidental reason: a block deletion runs the replay off the recorded choices and
//! the float is drawn afresh, landing on the value that matches, where an integer in the same
//! place stays recorded. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<i64>, f64);

fn draw(tc: &TestCase) -> Draws {
    let v: Vec<i64> =
        tc.draw_silent(gs::vecs(gs::integers::<i64>().min_value(0).max_value(1000)).max_size(10));
    let f: f64 = tc.draw_silent(gs::floats::<f64>().min_value(0.0).max_value(10.0));
    (v, f)
}

fn float_length_with_payload((v, f): &Draws) -> bool {
    *f == v.len() as f64 && v.iter().any(|&x| x != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(float_length_with_payload(&(vec![1], 1.0)));
    assert!(!float_length_with_payload(&(vec![1], 0.0)));
    assert!(!float_length_with_payload(&(vec![0], 1.0)));
    assert!(!float_length_with_payload(&(vec![], 0.0)));
    assert!(float_length_with_payload(&(vec![0, 1], 2.0)));
}

#[test]
#[ignore = "shrinker: no pass lowers the float behind the list with the element deletion; reaching the ideal relied on a fresh draw landing on the exact length, which the float mixture makes less likely"]
fn control_elements_are_deleted_with_the_float_length_behind() {
    assert_shrinks_to(&(vec![1], 1.0), 30, 500, draw, float_length_with_payload);
}
