mod common;

use common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::derive_generator;
use hegel::generators as gs;
use std::ops::Range;

#[derive(Debug)]
pub struct Person {
    pub name: String,
    pub age: u32,
}

derive_generator!(PersonGenerator for Person {
    name: String,
    age: u32,
});

#[derive(Debug)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

derive_generator!(PointGenerator for Point { x: i32, y: i32 });

type IntRange = Range<i32>;

derive_generator!(IntRangeGenerator for IntRange {
    start: i32,
    end: i32,
});

#[test]
fn test_derive_generator_new_uses_field_defaults() {
    check_can_generate_examples(PersonGenerator::new());
}

#[test]
fn test_derive_generator_default_impl() {
    let generator: PersonGenerator = Default::default();
    check_can_generate_examples(generator);
}

#[test]
fn test_derive_generator_two_invocations_do_not_collide() {
    check_can_generate_examples(PointGenerator::new());
}

#[test]
fn test_derive_generator_field_overrides() {
    assert_all_examples(
        PersonGenerator::new()
            .name(gs::from_regex("[A-Z][a-z]+"))
            .age(gs::integers::<u32>().min_value(18).max_value(65)),
        |person| {
            (18..=65).contains(&person.age)
                && person.name.len() >= 2
                && person.name.chars().next().unwrap().is_ascii_uppercase()
        },
    );
}

#[test]
fn test_derive_generator_all_builders() {
    assert_all_examples(
        PointGenerator::new()
            .x(gs::integers::<i32>().min_value(-5).max_value(5))
            .y(gs::integers::<i32>().min_value(100).max_value(200)),
        |point| (-5..=5).contains(&point.x) && (100..=200).contains(&point.y),
    );
}

#[test]
fn test_derive_generator_foreign_type() {
    assert_all_examples(
        IntRangeGenerator::new()
            .start(gs::integers::<i32>().min_value(0).max_value(10))
            .end(gs::integers::<i32>().min_value(20).max_value(30)),
        |range| (0..=10).contains(&range.start) && (20..=30).contains(&range.end),
    );
}
