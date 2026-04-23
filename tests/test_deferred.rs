mod common;

use common::utils::find_any;
use hegel::TestCase;
use hegel::generators::{self as gs, Generator};

#[hegel::test]
fn test_deferred_delegates_to_inner(tc: TestCase) {
    let d = gs::deferred();
    let g = d.generator();
    d.set(gs::integers::<i32>().min_value(0).max_value(10));
    let value = tc.draw(g);
    assert!((0..=10).contains(&value));
}

#[test]
fn test_deferred_can_generate_both_true_and_false() {
    let d = gs::deferred();
    let g = d.generator();
    d.set(gs::booleans());
    find_any(g.clone(), |v| *v);
    find_any(g, |v| !v);
}

#[hegel::test]
fn test_deferred_multiple_generators_share_definition(tc: TestCase) {
    let d = gs::deferred::<i32>();
    let g1 = d.generator();
    let g2 = d.generator();
    d.set(gs::integers().min_value(0).max_value(10));
    let v1 = tc.draw(g1);
    let v2 = tc.draw(g2);
    assert!((0..=10).contains(&v1));
    assert!((0..=10).contains(&v2));
}

#[test]
#[should_panic(expected = "has not been set")]
fn test_deferred_draw_before_set_panics() {
    hegel::hegel(|tc| {
        let d = gs::deferred::<bool>();
        let g = d.generator();
        tc.draw(g);
    });
}

#[hegel::test]
fn test_deferred_works_with_map(tc: TestCase) {
    let d = gs::deferred();
    let g = d.generator();
    d.set(gs::integers::<i32>().min_value(0).max_value(100));
    let value = tc.draw(g.map(|n| n * 2));
    assert!(value % 2 == 0);
    assert!((0..=200).contains(&value));
}

#[hegel::test]
fn test_deferred_self_recursive(tc: TestCase) {
    let d = gs::deferred::<Vec<bool>>();
    let g = d.generator();
    d.set(hegel::one_of!(
        gs::just(vec![]),
        g.clone().map(|mut v| {
            v.push(true);
            v
        }),
    ));
    let value = tc.draw(g);
    assert!(value.iter().all(|b| *b));
}

#[test]
fn test_deferred_mutual_recursion() {
    let x = gs::deferred::<i32>();
    let y = gs::deferred::<i32>();

    let x_gen = x.generator();
    let y_gen = y.generator();
    let x_draw = x.generator();

    y.set(hegel::one_of!(
        gs::integers::<i32>().min_value(0).max_value(10),
        x_gen,
    ));

    x.set(hegel::one_of!(
        gs::integers::<i32>().min_value(100).max_value(110),
        y_gen,
    ));

    find_any(x_draw.clone(), |v| (0..=10).contains(v));
    find_any(x_draw, |v| (100..=110).contains(v));
}
