mod common;

use common::utils::check_can_generate_examples;
use hegel::TestCase;
use hegel::generators::default;
use std::collections::HashMap;

#[test]
fn test_default_bool() {
    check_can_generate_examples(default::<bool>());
}

#[test]
fn test_default_string() {
    check_can_generate_examples(default::<String>());
}

#[test]
fn test_default_ints() {
    check_can_generate_examples(default::<i8>());
    check_can_generate_examples(default::<i16>());
    check_can_generate_examples(default::<i32>());
    check_can_generate_examples(default::<i64>());
    check_can_generate_examples(default::<u8>());
    check_can_generate_examples(default::<u16>());
    check_can_generate_examples(default::<u32>());
    check_can_generate_examples(default::<u64>());
    check_can_generate_examples(default::<i128>());
    check_can_generate_examples(default::<u128>());
    check_can_generate_examples(default::<isize>());
    check_can_generate_examples(default::<usize>());
}

#[test]
fn test_default_floats() {
    check_can_generate_examples(default::<f32>());
    check_can_generate_examples(default::<f64>());
}

#[test]
fn test_default_option() {
    check_can_generate_examples(default::<Option<i32>>());
    check_can_generate_examples(default::<Option<bool>>());
    check_can_generate_examples(default::<Option<String>>());
}

#[test]
fn test_default_vec() {
    check_can_generate_examples(default::<Vec<i32>>());
    check_can_generate_examples(default::<Vec<String>>());
    check_can_generate_examples(default::<Vec<bool>>());
}

#[test]
fn test_default_array() {
    check_can_generate_examples(default::<[bool; 2]>());
    check_can_generate_examples(default::<[i32; 5]>());
    check_can_generate_examples(default::<[String; 3]>());
    check_can_generate_examples(default::<[i32; 0]>());
}

#[test]
fn test_default_hashmap() {
    check_can_generate_examples(default::<HashMap<String, i32>>());
    check_can_generate_examples(default::<HashMap<String, bool>>());
}

#[test]
fn test_default_tuple() {
    check_can_generate_examples(default::<(i32, bool)>());
    check_can_generate_examples(default::<(i32, bool, String)>());
    check_can_generate_examples(default::<(i32, bool, String, f64)>());
}

#[test]
fn test_default_nested() {
    check_can_generate_examples(default::<Option<Vec<i32>>>());
    check_can_generate_examples(default::<Vec<Vec<i32>>>());
    check_can_generate_examples(default::<Vec<Option<bool>>>());
    check_can_generate_examples(default::<[[i32; 2]; 3]>());
    check_can_generate_examples(default::<Vec<(i32, bool)>>());
    check_can_generate_examples(default::<HashMap<String, Vec<i32>>>());
    check_can_generate_examples(default::<Option<(i32, String)>>());
    check_can_generate_examples(default::<[Option<i32>; 4]>());
}

#[hegel::test]
fn test_default_can_infer_through_draw(tc: TestCase) {
    // This doesn't test anything much at runtime. We are checking
    // that the type checker can infer the type parameter to default
    // rather than forcing us to write this as default::<i32>
    let _: i32 = tc.draw(default());
}
