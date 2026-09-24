//! Control: matched brackets around a payload, two deletions that only work together.
//!
//! String reading: `s = text().alphabet("()x").max_size(12)`, checked only on balanced strings,
//! fails iff a balanced string contains an `x`; ideal `"x"`. List reading:
//! `vecs(integers 0..=100).max_size(12)` with `0` opening, `1` closing and anything else payload;
//! ideal `[2]` (`reorder_spans` sorts `[0, 2, 1]` into `[0, 1, 2]`, whose `[0, 1]` is an
//! adjacent deletable pair). From `"(x)"` every single deletion unbalances the string, so the `(`
//! and its `)` must go in one move. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

fn draw_string(tc: &TestCase) -> String {
    tc.draw_silent(gs::text().alphabet("()x").max_size(12))
}

fn balanced_with_payload(s: &str) -> bool {
    let mut depth = 0i32;
    let mut payload = false;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => payload = true,
        }
    }
    depth == 0 && payload
}

fn draw_list(tc: &TestCase) -> Vec<i64> {
    tc.draw_silent(gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)).max_size(12))
}

fn list_balanced_with_payload(v: &[i64]) -> bool {
    let mut depth = 0i32;
    let mut payload = false;
    for &t in v {
        match t {
            0 => depth += 1,
            1 => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => payload = true,
        }
    }
    depth == 0 && payload
}

#[test]
fn the_ideals_fail_and_are_smallest() {
    assert!(balanced_with_payload("x"));
    assert!(!balanced_with_payload("("));
    assert!(!balanced_with_payload(")"));
    assert!(!balanced_with_payload(""));
    assert!(balanced_with_payload("(x)"));
    assert!(!balanced_with_payload("x)"));
    assert!(list_balanced_with_payload(&[2]));
    assert!(!list_balanced_with_payload(&[0]));
    assert!(!list_balanced_with_payload(&[1]));
    assert!(list_balanced_with_payload(&[0, 2, 1]));
}

#[test]
fn control_brackets_around_the_payload_are_deleted_together() {
    assert_shrinks_to(&"x".to_string(), 30, 100, draw_string, |s: &String| {
        balanced_with_payload(s)
    });
}

#[test]
fn control_list_brackets_around_the_payload_are_deleted_together() {
    assert_shrinks_to(&vec![2], 30, 100, draw_list, |v| {
        list_balanced_with_payload(v)
    });
}
