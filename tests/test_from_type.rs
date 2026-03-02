mod common;

use common::utils::check_can_generate_examples;
use hegel::generators::from_type;
use std::collections::HashMap;

#[test]
fn test_from_type_bool() {
    check_can_generate_examples(from_type::<bool>());
}

#[test]
fn test_from_type_string() {
    check_can_generate_examples(from_type::<String>());
}

#[test]
fn test_from_type_ints() {
    check_can_generate_examples(from_type::<i8>());
    check_can_generate_examples(from_type::<i16>());
    check_can_generate_examples(from_type::<i32>());
    check_can_generate_examples(from_type::<i64>());
    check_can_generate_examples(from_type::<u8>());
    check_can_generate_examples(from_type::<u16>());
    check_can_generate_examples(from_type::<u32>());
    check_can_generate_examples(from_type::<u64>());
    check_can_generate_examples(from_type::<i128>());
    check_can_generate_examples(from_type::<u128>());
    check_can_generate_examples(from_type::<isize>());
    check_can_generate_examples(from_type::<usize>());
}

#[test]
fn test_from_type_floats() {
    check_can_generate_examples(from_type::<f32>());
    check_can_generate_examples(from_type::<f64>());
}

#[test]
fn test_from_type_option() {
    check_can_generate_examples(from_type::<Option<i32>>());
    check_can_generate_examples(from_type::<Option<bool>>());
    check_can_generate_examples(from_type::<Option<String>>());
}

#[test]
fn test_from_type_vec() {
    check_can_generate_examples(from_type::<Vec<i32>>());
    check_can_generate_examples(from_type::<Vec<String>>());
    check_can_generate_examples(from_type::<Vec<bool>>());
}

#[test]
fn test_from_type_array() {
    check_can_generate_examples(from_type::<[bool; 2]>());
    check_can_generate_examples(from_type::<[i32; 5]>());
    check_can_generate_examples(from_type::<[String; 3]>());
    check_can_generate_examples(from_type::<[i32; 0]>());
}

#[test]
fn test_from_type_hashmap() {
    check_can_generate_examples(from_type::<HashMap<String, i32>>());
    check_can_generate_examples(from_type::<HashMap<String, bool>>());
}

#[test]
fn test_from_type_tuple() {
    check_can_generate_examples(from_type::<(i32, bool)>());
    check_can_generate_examples(from_type::<(i32, bool, String)>());
    check_can_generate_examples(from_type::<(i32, bool, String, f64)>());
}

#[test]
fn test_from_type_nested() {
    check_can_generate_examples(from_type::<Option<Vec<i32>>>());
    check_can_generate_examples(from_type::<Vec<Vec<i32>>>());
    check_can_generate_examples(from_type::<Vec<Option<bool>>>());
    check_can_generate_examples(from_type::<[[i32; 2]; 3]>());
    check_can_generate_examples(from_type::<Vec<(i32, bool)>>());
    check_can_generate_examples(from_type::<HashMap<String, Vec<i32>>>());
    check_can_generate_examples(from_type::<Option<(i32, String)>>());
    check_can_generate_examples(from_type::<[Option<i32>; 4]>());
}
