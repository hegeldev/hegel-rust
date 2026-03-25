use crate::common::utils::find_any;
use hegel::generators::{self, Generator};

// Nested one_of: can reach all 8 branches

fn nested_one_of() -> impl Generator<i64> {
    hegel::one_of!(
        generators::just(0i64),
        hegel::one_of!(
            generators::just(1i64),
            generators::just(2i64),
            hegel::one_of!(
                generators::just(3i64),
                generators::just(4i64),
                hegel::one_of!(
                    generators::just(5i64),
                    generators::just(6i64),
                    generators::just(7i64)
                )
            )
        )
    )
}

#[test]
fn test_one_of_flattens_branches_0() {
    find_any(nested_one_of(), |&x| x == 0);
}

#[test]
fn test_one_of_flattens_branches_1() {
    find_any(nested_one_of(), |&x| x == 1);
}

#[test]
fn test_one_of_flattens_branches_2() {
    find_any(nested_one_of(), |&x| x == 2);
}

#[test]
fn test_one_of_flattens_branches_3() {
    find_any(nested_one_of(), |&x| x == 3);
}

#[test]
fn test_one_of_flattens_branches_4() {
    find_any(nested_one_of(), |&x| x == 4);
}

#[test]
fn test_one_of_flattens_branches_5() {
    find_any(nested_one_of(), |&x| x == 5);
}

#[test]
fn test_one_of_flattens_branches_6() {
    find_any(nested_one_of(), |&x| x == 6);
}

#[test]
fn test_one_of_flattens_branches_7() {
    find_any(nested_one_of(), |&x| x == 7);
}

// Nested one_of with map: generates {1, 4, 6, 16, 20, 24, 28, 32}

fn nested_one_of_with_map() -> impl Generator<i64> {
    hegel::one_of!(
        generators::just(1i64),
        hegel::one_of!(
            hegel::one_of!(generators::just(2i64), generators::just(3i64)).map(|x| x * 2),
            hegel::one_of!(
                hegel::one_of!(generators::just(4i64), generators::just(5i64)).map(|x| x * 2),
                hegel::one_of!(
                    generators::just(6i64),
                    generators::just(7i64),
                    generators::just(8i64)
                )
                .map(|x| x * 2)
            )
            .map(|x| x * 2)
        )
    )
}

#[test]
fn test_one_of_flattens_map_branches_1() {
    find_any(nested_one_of_with_map(), |&x| x == 1);
}

#[test]
fn test_one_of_flattens_map_branches_4() {
    find_any(nested_one_of_with_map(), |&x| x == 4);
}

#[test]
fn test_one_of_flattens_map_branches_6() {
    find_any(nested_one_of_with_map(), |&x| x == 6);
}

#[test]
fn test_one_of_flattens_map_branches_16() {
    find_any(nested_one_of_with_map(), |&x| x == 16);
}

#[test]
fn test_one_of_flattens_map_branches_20() {
    find_any(nested_one_of_with_map(), |&x| x == 20);
}

#[test]
fn test_one_of_flattens_map_branches_24() {
    find_any(nested_one_of_with_map(), |&x| x == 24);
}

#[test]
fn test_one_of_flattens_map_branches_28() {
    find_any(nested_one_of_with_map(), |&x| x == 28);
}

#[test]
fn test_one_of_flattens_map_branches_32() {
    find_any(nested_one_of_with_map(), |&x| x == 32);
}

// Nested one_of with flatmap: generates Vec<()> of length 0-7

fn nested_one_of_with_flatmap() -> impl Generator<Vec<()>> {
    generators::just(()).flat_map(|x| {
        hegel::one_of!(
            generators::just(vec![x; 0]),
            generators::just(vec![x; 1]),
            hegel::one_of!(
                generators::just(vec![x; 2]),
                generators::just(vec![x; 3]),
                hegel::one_of!(
                    generators::just(vec![x; 4]),
                    generators::just(vec![x; 5]),
                    hegel::one_of!(generators::just(vec![x; 6]), generators::just(vec![x; 7]))
                )
            )
        )
    })
}

#[test]
fn test_one_of_flattens_flatmap_branches_0() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.is_empty());
}

#[test]
fn test_one_of_flattens_flatmap_branches_1() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.len() == 1);
}

#[test]
fn test_one_of_flattens_flatmap_branches_2() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.len() == 2);
}

#[test]
fn test_one_of_flattens_flatmap_branches_3() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.len() == 3);
}

#[test]
fn test_one_of_flattens_flatmap_branches_4() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.len() == 4);
}

#[test]
fn test_one_of_flattens_flatmap_branches_5() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.len() == 5);
}

#[test]
fn test_one_of_flattens_flatmap_branches_6() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.len() == 6);
}

#[test]
fn test_one_of_flattens_flatmap_branches_7() {
    find_any(nested_one_of_with_flatmap(), |x: &Vec<()>| x.len() == 7);
}

// Nested one_of with filter: generates even integers {0, 2, 4, 6}

fn nested_one_of_with_filter() -> impl Generator<i64> {
    hegel::one_of!(
        generators::just(0i64),
        generators::just(1i64),
        hegel::one_of!(
            generators::just(2i64),
            generators::just(3i64),
            hegel::one_of!(
                generators::just(4i64),
                generators::just(5i64),
                hegel::one_of!(generators::just(6i64), generators::just(7i64))
            )
        )
    )
    .filter(|&x| x % 2 == 0)
}

#[test]
fn test_one_of_flattens_filter_branches_0() {
    find_any(nested_one_of_with_filter(), |&x| x == 0);
}

#[test]
fn test_one_of_flattens_filter_branches_2() {
    find_any(nested_one_of_with_filter(), |&x| x == 2);
}

#[test]
fn test_one_of_flattens_filter_branches_4() {
    find_any(nested_one_of_with_filter(), |&x| x == 4);
}

#[test]
fn test_one_of_flattens_filter_branches_6() {
    find_any(nested_one_of_with_filter(), |&x| x == 6);
}
