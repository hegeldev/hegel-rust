use std::collections::HashSet;

use crate::common::utils::minimal;
use hegel::generators::{self, Generator};

// one_of with same-type generators (integers only).
#[test]
fn test_minimize_one_of_integers() {
    for _ in 0..10 {
        let result = minimal(
            hegel::one_of!(
                generators::integers::<i64>(),
                generators::integers::<i64>().min_value(100).max_value(200),
            ),
            |_| true,
        );
        assert_eq!(result, 0);
    }
}

// Mixed types via enum: minimal(integers() | text() | booleans()) in (0, "", False)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum IntOrTextOrBool {
    Int(i64),
    Text(String),
    Bool(bool),
}

#[test]
fn test_minimize_one_of_mixed() {
    for _ in 0..10 {
        let result = minimal(
            hegel::one_of!(
                generators::integers::<i64>().map(IntOrTextOrBool::Int),
                generators::text().map(IntOrTextOrBool::Text),
                generators::booleans().map(IntOrTextOrBool::Bool)
            ),
            |_| true,
        );
        assert!(
            result == IntOrTextOrBool::Int(0)
                || result == IntOrTextOrBool::Text(String::new())
                || result == IntOrTextOrBool::Bool(false),
            "Expected Int(0), Text(\"\"), or Bool(false), got {:?}",
            result
        );
    }
}

// Mixed list: minimal(lists(integers() | text()), len >= 10) subset of {0, ""}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum IntOrText {
    Int(i64),
    Text(String),
}

#[test]
fn test_minimize_mixed_list() {
    let result = minimal(
        generators::vecs(hegel::one_of!(
            generators::integers::<i64>().map(IntOrText::Int),
            generators::text().map(IntOrText::Text)
        )),
        |x: &Vec<IntOrText>| x.len() >= 10,
    );
    assert_eq!(result.len(), 10);
    let unique: HashSet<_> = result.iter().collect();
    let allowed: HashSet<IntOrText> =
        HashSet::from([IntOrText::Int(0), IntOrText::Text(String::new())]);
    for item in &unique {
        assert!(
            allowed.contains(item),
            "Unexpected item in minimal mixed list: {:?}",
            item
        );
    }
}

// Mixed flatmap: booleans().flatmap(b => booleans() if b else text())
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum BoolOrText {
    Bool(bool),
    Text(String),
}

#[hegel::composite]
fn bool_or_text_via_flatmap(tc: hegel::TestCase) -> BoolOrText {
    let b: bool = tc.draw(generators::booleans());
    if b {
        BoolOrText::Bool(tc.draw(generators::booleans()))
    } else {
        BoolOrText::Text(tc.draw(generators::text()))
    }
}

#[test]
fn test_mixed_list_flatmap() {
    let result = minimal(
        generators::vecs(bool_or_text_via_flatmap()),
        |ls: &Vec<BoolOrText>| {
            let bools = ls
                .iter()
                .filter(|x| matches!(x, BoolOrText::Bool(_)))
                .count();
            let texts = ls
                .iter()
                .filter(|x| matches!(x, BoolOrText::Text(_)))
                .count();
            bools >= 3 && texts >= 3
        },
    );
    assert_eq!(result.len(), 6);
    let unique: HashSet<_> = result.iter().collect();
    assert_eq!(
        unique,
        HashSet::from([&BoolOrText::Bool(false), &BoolOrText::Text(String::new())])
    );
}

// one_of shrinks towards earlier branches.
#[test]
fn test_one_of_slip() {
    let result = minimal(
        hegel::one_of!(
            generators::integers::<i64>().min_value(101).max_value(200),
            generators::integers::<i64>().min_value(0).max_value(100),
        ),
        |_| true,
    );
    assert_eq!(result, 101);
}
