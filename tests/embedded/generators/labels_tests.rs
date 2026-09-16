//! The frontend computes labels itself (at compile time, mostly) rather than
//! calling the engine; these check that it computes the same labels the
//! engine's `hegel_label_from_name` / `hegel_label_combine` would.

use super::*;
use crate::ffi::sys as hegel_c;
use std::ffi::CString;
use std::ptr;

fn engine_label_from_name(name: &str) -> u64 {
    let name = CString::new(name).unwrap();
    let mut label = 0u64;
    let rc = unsafe { hegel_c::hegel_label_from_name(ptr::null_mut(), name.as_ptr(), &mut label) };
    assert_eq!(rc, hegel_c::hegel_result_t::HEGEL_OK);
    label
}

fn engine_combine_labels(labels: &[u64]) -> u64 {
    let mut label = 0u64;
    let rc = unsafe {
        hegel_c::hegel_label_combine(ptr::null_mut(), labels.as_ptr(), labels.len(), &mut label)
    };
    assert_eq!(rc, hegel_c::hegel_result_t::HEGEL_OK);
    label
}

#[test]
fn label_from_name_matches_the_engine() {
    let long = "x".repeat(1000);
    for name in [
        "",
        "a",
        "hegel.integer",
        "mycrate.pairs",
        "ünïcödé ✓",
        long.as_str(),
    ] {
        assert_eq!(
            label_from_name(name),
            engine_label_from_name(name),
            "{name:?}"
        );
    }
}

#[test]
fn label_from_name_matches_the_published_fnv1a_vectors() {
    assert_eq!(label_from_name(""), 0xcbf29ce484222325);
    assert_eq!(label_from_name("a"), 0xaf63dc4c8601ec8c);
    assert_eq!(label_from_name("foobar"), 0x85944171f73967e8);
}

#[test]
fn combine_labels_matches_the_engine() {
    let a = label_from_name("a");
    let b = label_from_name("b");
    for labels in [
        &[][..],
        &[a],
        &[a, b],
        &[b, a],
        &[a, a, a],
        &[u64::MAX, 0, 1 << 63],
    ] {
        assert_eq!(
            combine_labels(labels),
            engine_combine_labels(labels),
            "{labels:?}"
        );
    }
}

#[test]
fn combining_distinguishes_order_and_arity() {
    let a = label_from_name("a");
    let b = label_from_name("b");
    assert_ne!(combine_labels(&[a, b]), combine_labels(&[b, a]));
    assert_ne!(combine_labels(&[a]), a);
    assert_ne!(combine_labels(&[a]), combine_labels(&[a, a]));
    assert_ne!(combine_labels(&[]), combine_labels(&[a]));
}
