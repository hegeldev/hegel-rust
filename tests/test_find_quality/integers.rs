use crate::common::utils::{find_any, minimal};
use hegel::generators;

// From test_discovery_ability.py

#[test]
fn test_can_produce_zero() {
    find_any(generators::integers::<i64>(), |&x| x == 0);
}

#[test]
fn test_can_produce_large_magnitude_integers() {
    find_any(generators::integers::<i64>(), |&x| x.abs() > 1000);
}

#[test]
fn test_can_produce_large_positive_integers() {
    find_any(generators::integers::<i64>(), |&x| x > 1000);
}

#[test]
fn test_can_produce_large_negative_integers() {
    find_any(generators::integers::<i64>(), |&x| x < -1000);
}

#[test]
fn test_integers_are_usually_non_zero() {
    find_any(generators::integers::<i64>(), |&x| x != 0);
}

#[test]
fn test_integers_are_sometimes_zero() {
    find_any(generators::integers::<i64>(), |&x| x == 0);
}

#[test]
fn test_integers_are_often_small() {
    find_any(generators::integers::<i64>(), |&x| x.abs() <= 100);
}

#[test]
fn test_integers_are_often_small_but_not_that_small() {
    find_any(generators::integers::<i64>(), |&x| {
        (50..=255).contains(&x.abs())
    });
}

#[test]
fn test_ints_can_occasionally_be_really_large() {
    find_any(generators::integers::<i64>(), |&x| x >= i64::MAX / 2);
}

// From test_simple_numbers.py — integer shrinking/findability

#[test]
fn test_minimize_negative_int() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x < 0), -1);
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x < -1), -2);
}

#[test]
fn test_positive_negative_int() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x > 0), 1);
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x > 1), 2);
}

// Boundary values: 2^i, 2^i - 1, 2^i + 1, 10^i

#[test]
fn test_minimizes_int_down_to_boundary_1() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 1), 1);
}

#[test]
fn test_minimizes_int_down_to_boundary_2() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 2), 2);
}

#[test]
fn test_minimizes_int_down_to_boundary_3() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 3), 3);
}

#[test]
fn test_minimizes_int_down_to_boundary_4() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 4), 4);
}

#[test]
fn test_minimizes_int_down_to_boundary_7() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 7), 7);
}

#[test]
fn test_minimizes_int_down_to_boundary_8() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 8), 8);
}

#[test]
fn test_minimizes_int_down_to_boundary_15() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 15), 15);
}

#[test]
fn test_minimizes_int_down_to_boundary_16() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 16), 16);
}

#[test]
fn test_minimizes_int_down_to_boundary_31() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 31), 31);
}

#[test]
fn test_minimizes_int_down_to_boundary_32() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 32), 32);
}

#[test]
fn test_minimizes_int_down_to_boundary_63() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 63), 63);
}

#[test]
fn test_minimizes_int_down_to_boundary_64() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 64), 64);
}

#[test]
fn test_minimizes_int_down_to_boundary_100() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 100), 100);
}

#[test]
fn test_minimizes_int_down_to_boundary_127() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 127), 127);
}

#[test]
fn test_minimizes_int_down_to_boundary_128() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 128), 128);
}

#[test]
fn test_minimizes_int_down_to_boundary_255() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 255), 255);
}

#[test]
fn test_minimizes_int_down_to_boundary_256() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 256), 256);
}

#[test]
fn test_minimizes_int_down_to_boundary_511() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 511), 511);
}

#[test]
fn test_minimizes_int_down_to_boundary_512() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 512), 512);
}

#[test]
fn test_minimizes_int_down_to_boundary_1000() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x >= 1000), 1000);
}

#[test]
fn test_minimizes_int_down_to_boundary_10000() {
    assert_eq!(
        minimal(generators::integers::<i64>(), |&x| x >= 10000),
        10000
    );
}

#[test]
fn test_minimizes_int_down_to_boundary_100000() {
    assert_eq!(
        minimal(generators::integers::<i64>(), |&x| x >= 100000),
        100000
    );
}

// Upward boundary tests (negative)

#[test]
fn test_minimizes_int_up_to_boundary_1() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x <= -1), -1);
}

#[test]
fn test_minimizes_int_up_to_boundary_16() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x <= -16), -16);
}

#[test]
fn test_minimizes_int_up_to_boundary_128() {
    assert_eq!(minimal(generators::integers::<i64>(), |&x| x <= -128), -128);
}

#[test]
fn test_minimizes_int_up_to_boundary_1000() {
    assert_eq!(
        minimal(generators::integers::<i64>(), |&x| x <= -1000),
        -1000
    );
}

// Bounded range tests

#[test]
fn test_minimizes_ints_from_down_to_boundary_16() {
    assert_eq!(
        minimal(generators::integers::<i64>().min_value(6), |&x| x >= 16),
        16
    );
    assert_eq!(
        minimal(generators::integers::<i64>().min_value(16), |_| true),
        16
    );
}

#[test]
fn test_minimizes_ints_from_down_to_boundary_128() {
    assert_eq!(
        minimal(generators::integers::<i64>().min_value(118), |&x| x >= 128),
        128
    );
    assert_eq!(
        minimal(generators::integers::<i64>().min_value(128), |_| true),
        128
    );
}

#[test]
fn test_minimizes_ints_from_down_to_boundary_1000() {
    assert_eq!(
        minimal(generators::integers::<i64>().min_value(990), |&x| x >= 1000),
        1000
    );
    assert_eq!(
        minimal(generators::integers::<i64>().min_value(1000), |_| true),
        1000
    );
}

#[test]
fn test_minimizes_negative_integer_range_upwards() {
    assert_eq!(
        minimal(
            generators::integers::<i64>().min_value(-10).max_value(-1),
            |_| true
        ),
        -1
    );
}

#[test]
fn test_minimizes_integer_range_to_boundary_16() {
    assert_eq!(
        minimal(
            generators::integers::<i64>().min_value(16).max_value(116),
            |_| true
        ),
        16
    );
}

#[test]
fn test_minimizes_integer_range_to_boundary_128() {
    assert_eq!(
        minimal(
            generators::integers::<i64>().min_value(128).max_value(228),
            |_| true
        ),
        128
    );
}

#[test]
fn test_single_integer_range_is_range() {
    assert_eq!(
        minimal(
            generators::integers::<i64>().min_value(1).max_value(1),
            |_| true
        ),
        1
    );
}

#[test]
fn test_minimal_small_number_in_large_range() {
    assert_eq!(
        minimal(
            generators::integers::<i64>()
                .min_value(-(1i64 << 32))
                .max_value(1i64 << 32),
            |&x| x >= 101
        ),
        101
    );
}

#[test]
fn test_minimizes_lists_of_negative_ints_up_to_boundary() {
    let result = minimal(
        generators::vecs(generators::integers::<i64>()).min_size(10),
        |x: &Vec<i64>| x.iter().filter(|&&t| t <= -1).count() >= 10,
    );
    assert_eq!(result, vec![-1; 10]);
}
