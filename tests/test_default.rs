mod common;

use common::project::TempRustProject;
use common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::generators as gs;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

use crate::common::utils::find_any;

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
fn test_default_hashset() {
    check_can_generate_examples(gs::default::<std::collections::HashSet<i32>>());
    check_can_generate_examples(gs::default::<std::collections::HashSet<String>>());
}

#[test]
fn test_default_pathbuf() {
    check_can_generate_examples(gs::default::<PathBuf>());

    // Check that some paths are valid UTF-8
    find_any(gs::default::<PathBuf>(), |p| p.to_str().is_some());
    // Check that some paths are not UTF-8
    find_any(gs::default::<PathBuf>(), |p| p.to_str().is_none());
}

#[test]
fn test_default_ipaddress() {
    check_can_generate_examples(gs::default::<IpAddr>());
    check_can_generate_examples(gs::default::<Ipv4Addr>());
    check_can_generate_examples(gs::default::<Ipv6Addr>());
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

#[test]
fn test_default_supports_primitive_builder() {
    let g = gs::default::<u32>().min_value(10).max_value(20);
    assert_all_examples(g, |n: &u32| *n >= 10 && *n <= 20);
}

#[test]
fn test_default_cant_infer_through_draw() {
    TempRustProject::new()
        .main_file(
            r#"
use hegel::generators as gs;

fn _check(tc: &hegel::TestCase) {
    let _: i32 = tc.draw(gs::default());
}

fn main() {}
"#,
        )
        .expect_failure("type annotations needed")
        .cargo_run(&[]);
}
