//! Ported from hypothesis-python/tests/cover/test_provisional_strategies.py
//!
//! Individually-skipped tests:
//! - `test_url_fragments_contain_legal_chars` — imports the private
//!   `_url_fragments_strategy` strategy object and the
//!   `FRAGMENT_SAFE_CHARACTERS` constant from `hypothesis.provisional`;
//!   hegel-rust exposes neither a URL-fragment generator nor the
//!   fragment-safe-characters set as public API.
//! - `test_invalid_domain_arguments[max_length=-1|4.0]` and every
//!   `max_element_length` row — hegel-rust's `DomainGenerator::max_length`
//!   takes `usize` (so negative and float values are unrepresentable) and
//!   exposes no `max_element_length` setter, leaving only the
//!   `max_length ∈ {0, 3, 256}` invalid cases portable.
//! - `test_valid_domains_arguments[max_element_length=...]` rows — same
//!   gap; only `max_length ∈ {None, 4, 8, 255}` is portable.

use std::collections::HashSet;

use regex::Regex;

use crate::common::utils::{
    assert_all_examples, check_can_generate_examples, expect_panic, find_any,
};
use hegel::generators::{self as gs, Generator};

fn url_allowed_chars() -> HashSet<char> {
    ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain("$-_.+!*'(),~%/".chars())
        .collect()
}

#[test]
fn test_is_url() {
    let allowed = url_allowed_chars();
    let mut fragment_allowed = allowed.clone();
    fragment_allowed.insert('?');
    let hex_pair = Regex::new(r"^[0-9A-Fa-f]{2}").unwrap();

    assert_all_examples(gs::urls(), move |url: &String| {
        let url_schemeless = match url.split_once("://") {
            Some((_, rest)) => rest,
            None => return false,
        };
        let (domain_path, fragment) = match url_schemeless.split_once('#') {
            Some((dp, fr)) => (dp, fr),
            None => (url_schemeless, ""),
        };
        let path = domain_path.split_once('/').map_or("", |(_, p)| p);

        if !path.chars().all(|c| allowed.contains(&c)) {
            return false;
        }
        for after_perc in path.split('%').skip(1) {
            if !hex_pair.is_match(after_perc) {
                return false;
            }
        }

        if !fragment.chars().all(|c| fragment_allowed.contains(&c)) {
            return false;
        }
        for after_perc in fragment.split('%').skip(1) {
            if !hex_pair.is_match(after_perc) {
                return false;
            }
        }
        true
    });
}

#[test]
fn test_invalid_domain_arguments() {
    for max_length in [0_usize, 3, 256] {
        expect_panic(
            move || {
                gs::domains().max_length(max_length).as_basic();
            },
            "max_length must be between 4 and 255",
        );
    }
}

#[test]
fn test_valid_domains_arguments() {
    check_can_generate_examples(gs::domains());
    for max_length in [4_usize, 8, 255] {
        check_can_generate_examples(gs::domains().max_length(max_length));
    }
}

#[test]
fn test_find_any_non_empty_domains() {
    find_any(gs::domains(), |s: &String| !s.is_empty());
}

#[test]
fn test_find_any_non_empty_urls() {
    find_any(gs::urls(), |s: &String| !s.is_empty());
}
