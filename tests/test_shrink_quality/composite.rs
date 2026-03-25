use crate::common::utils::minimal;
use hegel::generators;

#[hegel::composite]
fn int_pair(tc: hegel::TestCase, lo: i64, hi: i64) -> (i64, i64) {
    let a = tc.draw(generators::integers::<i64>().min_value(lo).max_value(hi));
    let b = tc.draw(generators::integers::<i64>().min_value(lo).max_value(hi));
    (a, b)
}

#[test]
fn test_sum_of_pair() {
    assert_eq!(
        minimal(int_pair(0, 1000), |x: &(i64, i64)| x.0 + x.1 > 1000),
        (1, 1000)
    );
}

#[hegel::composite]
fn separated_sum(tc: hegel::TestCase) -> (i64, i64) {
    let n1 = tc.draw(generators::integers::<i64>().min_value(0).max_value(1000));
    let _ = tc.draw(generators::text());
    let _ = tc.draw(generators::booleans());
    let _ = tc.draw(generators::integers::<i64>());
    let n2 = tc.draw(generators::integers::<i64>().min_value(0).max_value(1000));
    (n1, n2)
}

#[test]
fn test_sum_of_pair_separated() {
    assert_eq!(
        minimal(separated_sum(), |x: &(i64, i64)| x.0 + x.1 > 1000),
        (1, 1000)
    );
}

#[test]
fn test_minimize_dict_of_booleans() {
    let result = minimal(
        generators::tuples!(generators::booleans(), generators::booleans()),
        |x: &(bool, bool)| x.0 || x.1,
    );
    assert!(!(result.0 && result.1));
    assert!(result.0 || result.1);
}

#[hegel::composite]
fn int_struct(tc: hegel::TestCase) -> (i64, i64) {
    let a = tc.draw(generators::integers::<i64>());
    let b = tc.draw(generators::integers::<i64>());
    (a, b)
}

#[test]
fn test_minimize_namedtuple() {
    let tab = minimal(int_struct(), |x: &(i64, i64)| x.0 < x.1);
    assert_eq!(tab.1, tab.0 + 1);
}
