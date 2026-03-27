mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::TestCase;
use hegel::generators::{self as gs, Generator};

// tuples0 (unit)

#[hegel::test]
fn test_tuple0_basic(tc: TestCase) {
    let _: () = tc.draw(gs::tuples!());
}

#[test]
fn test_tuple0_all_examples() {
    assert_all_examples(gs::tuples!(), |_| true);
}

#[test]
fn test_tuple0_default_generator() {
    assert_all_examples(gs::default::<()>(), |_| true);
}

// tuples1

#[hegel::test]
fn test_tuple1_basic(tc: TestCase) {
    let (a,): (i32,) = tc.draw(gs::tuples!(gs::integers(),));
    let _ = a;
}

#[hegel::test]
fn test_tuple1_respects_bounds(tc: TestCase) {
    let (a,): (i32,) = tc.draw(gs::tuples!(gs::integers().min_value(0).max_value(10),));
    assert!((0..=10).contains(&a));
}

// tuples2

#[hegel::test]
fn test_tuple2_basic(tc: TestCase) {
    let (a, b): (i32, bool) = tc.draw(gs::tuples!(gs::integers(), gs::booleans(),));
    let _ = (a, b);
}

#[hegel::test]
fn test_tuple2_respects_bounds(tc: TestCase) {
    let (a, b): (i32, i32) = tc.draw(gs::tuples!(
        gs::integers().min_value(0).max_value(10),
        gs::integers().min_value(100).max_value(200),
    ));
    assert!((0..=10).contains(&a));
    assert!((100..=200).contains(&b));
}

// tuples3

#[hegel::test]
fn test_tuple3_basic(tc: TestCase) {
    let (a, b, c): (i32, String, bool) =
        tc.draw(gs::tuples!(gs::integers(), gs::text(), gs::booleans(),));
    let _ = (a, b, c);
}

#[hegel::test]
fn test_tuple3_respects_bounds(tc: TestCase) {
    let (a, b, c): (i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers().min_value(0).max_value(10),
        gs::integers().min_value(20).max_value(30),
        gs::integers().min_value(40).max_value(50),
    ));
    assert!((0..=10).contains(&a));
    assert!((20..=30).contains(&b));
    assert!((40..=50).contains(&c));
}

// tuples4

#[hegel::test]
fn test_tuple4_basic(tc: TestCase) {
    let (a, b, c, d): (i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers().min_value(0).max_value(10),
        gs::integers().min_value(0).max_value(10),
        gs::integers().min_value(0).max_value(10),
        gs::integers().min_value(0).max_value(10),
    ));
    assert!((0..=10).contains(&a));
    assert!((0..=10).contains(&b));
    assert!((0..=10).contains(&c));
    assert!((0..=10).contains(&d));
}

// tuples5

#[hegel::test]
fn test_tuple5_basic(tc: TestCase) {
    let t: (i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
    let _ = t;
}

// larger arities compile and run

#[hegel::test]
fn test_tuple6(tc: TestCase) {
    let _: (i32, i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
}

#[hegel::test]
fn test_tuple7(tc: TestCase) {
    let _: (i32, i32, i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
}

#[hegel::test]
fn test_tuple8(tc: TestCase) {
    let _: (i32, i32, i32, i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
}

#[hegel::test]
fn test_tuple9(tc: TestCase) {
    let _: (i32, i32, i32, i32, i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
}

#[hegel::test]
fn test_tuple10(tc: TestCase) {
    let _: (i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
}

#[hegel::test]
fn test_tuple11(tc: TestCase) {
    let _: (i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
}

#[hegel::test]
fn test_tuple12(tc: TestCase) {
    let _: (i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) = tc.draw(gs::tuples!(
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
        gs::integers(),
    ));
}

// mapped tuples

#[hegel::test]
fn test_tuple2_with_mapped_elements(tc: TestCase) {
    let (a, b): (i32, i32) = tc.draw(gs::tuples!(
        gs::integers::<i32>()
            .min_value(i32::MIN / 2)
            .max_value(i32::MAX / 2)
            .map(|x| x * 2),
        gs::integers::<i32>()
            .min_value(0)
            .max_value(100)
            .map(|x| x + 1),
    ));
    assert!(a % 2 == 0);
    assert!((1..=101).contains(&b));
}

// mixed types

#[hegel::test]
fn test_tuple_mixed_types(tc: TestCase) {
    let (n, s, b, f): (i32, String, bool, f64) = tc.draw(gs::tuples!(
        gs::integers().min_value(0).max_value(100),
        gs::text().max_size(10),
        gs::booleans(),
        gs::floats(),
    ));
    assert!((0..=100).contains(&n));
    assert!(s.len() <= 40); // max_size is in chars, UTF-8 can expand
    let _ = (b, f);
}

// tuples in collections

#[hegel::test]
fn test_vec_of_tuples(tc: TestCase) {
    let vec: Vec<(i32, bool)> = tc.draw(
        gs::vecs(gs::tuples!(
            gs::integers::<i32>().min_value(0).max_value(100),
            gs::booleans(),
        ))
        .max_size(10),
    );
    for &(n, _b) in &vec {
        assert!((0..=100).contains(&n));
    }
}

// tuple can find specific values

#[test]
fn test_tuple2_can_find_both_true_and_false() {
    find_any(gs::tuples!(gs::booleans(), gs::booleans()), |(a, b)| {
        *a && !*b
    });
    find_any(gs::tuples!(gs::booleans(), gs::booleans()), |(a, b)| {
        !*a && *b
    });
}

// assert_all_examples with tuples

#[test]
fn test_tuple2_all_examples_in_bounds() {
    assert_all_examples(
        gs::tuples!(
            gs::integers::<i32>().min_value(0).max_value(10),
            gs::integers::<i32>().min_value(0).max_value(10),
        ),
        |(a, b)| (0..=10).contains(a) && (0..=10).contains(b),
    );
}

#[hegel::test]
fn test_tuple2_with_non_basic_element(tc: TestCase) {
    // flat_map removes as_basic(), forcing the tuple non-basic fallback path
    let (a, b) = tc.draw(generators::tuples!(
        generators::integers::<i32>().min_value(0).max_value(10),
        generators::integers::<usize>()
            .min_value(1)
            .max_value(3)
            .flat_map(|n| generators::text().min_size(n).max_size(n)),
    ));
    assert!((0..=10).contains(&a));
    assert!((1..=3).contains(&b.chars().count()));
}

#[test]
fn test_tuple3_all_examples_in_bounds() {
    assert_all_examples(
        gs::tuples!(
            gs::integers::<i32>().min_value(-5).max_value(5),
            gs::integers::<i32>().min_value(10).max_value(20),
            gs::integers::<i32>().min_value(100).max_value(200),
        ),
        |(a, b, c)| (-5..=5).contains(a) && (10..=20).contains(b) && (100..=200).contains(c),
    );
}
