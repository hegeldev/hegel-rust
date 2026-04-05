mod common;

use common::utils::check_can_generate_examples;
use hegel::TestCase;
use hegel::generators as gs;
use std::collections::HashMap;

#[test]
fn test_default_bool() {
    check_can_generate_examples(gs::default::<bool>());
}

#[test]
fn test_default_string() {
    check_can_generate_examples(gs::default::<String>());
}

#[test]
fn test_default_char() {
    check_can_generate_examples(gs::default::<char>());
}

#[test]
fn test_default_ints() {
    check_can_generate_examples(gs::default::<i8>());
    check_can_generate_examples(gs::default::<i16>());
    check_can_generate_examples(gs::default::<i32>());
    check_can_generate_examples(gs::default::<i64>());
    check_can_generate_examples(gs::default::<u8>());
    check_can_generate_examples(gs::default::<u16>());
    check_can_generate_examples(gs::default::<u32>());
    check_can_generate_examples(gs::default::<u64>());
    check_can_generate_examples(gs::default::<i128>());
    check_can_generate_examples(gs::default::<u128>());
    check_can_generate_examples(gs::default::<isize>());
    check_can_generate_examples(gs::default::<usize>());
}

#[test]
fn test_default_floats() {
    check_can_generate_examples(gs::default::<f32>());
    check_can_generate_examples(gs::default::<f64>());
}

#[test]
fn test_default_option() {
    check_can_generate_examples(gs::default::<Option<i32>>());
    check_can_generate_examples(gs::default::<Option<bool>>());
    check_can_generate_examples(gs::default::<Option<String>>());
}

#[test]
fn test_default_vec() {
    check_can_generate_examples(gs::default::<Vec<i32>>());
    check_can_generate_examples(gs::default::<Vec<String>>());
    check_can_generate_examples(gs::default::<Vec<bool>>());
}

#[test]
fn test_default_array() {
    check_can_generate_examples(gs::default::<[bool; 2]>());
    check_can_generate_examples(gs::default::<[i32; 5]>());
    check_can_generate_examples(gs::default::<[String; 3]>());
    check_can_generate_examples(gs::default::<[i32; 0]>());
}

#[test]
fn test_default_hashmap() {
    check_can_generate_examples(gs::default::<HashMap<String, i32>>());
    check_can_generate_examples(gs::default::<HashMap<String, bool>>());
}

#[test]
fn test_default_tuple() {
    check_can_generate_examples(gs::default::<(i32, bool)>());
    check_can_generate_examples(gs::default::<(i32, bool, String)>());
    check_can_generate_examples(gs::default::<(i32, bool, String, f64)>());
}

#[test]
fn test_default_nested() {
    check_can_generate_examples(gs::default::<Option<Vec<i32>>>());
    check_can_generate_examples(gs::default::<Vec<Vec<i32>>>());
    check_can_generate_examples(gs::default::<Vec<Option<bool>>>());
    check_can_generate_examples(gs::default::<[[i32; 2]; 3]>());
    check_can_generate_examples(gs::default::<Vec<(i32, bool)>>());
    check_can_generate_examples(gs::default::<HashMap<String, Vec<i32>>>());
    check_can_generate_examples(gs::default::<Option<(i32, String)>>());
    check_can_generate_examples(gs::default::<[Option<i32>; 4]>());
}

#[hegel::test]
fn test_default_can_infer_through_draw(tc: TestCase) {
    // This doesn't test anything much at runtime. We are checking
    // that the type checker can infer the type parameter to default
    // rather than forcing us to write this as gs::default::<i32>
    let _: i32 = tc.draw(gs::default());
}
