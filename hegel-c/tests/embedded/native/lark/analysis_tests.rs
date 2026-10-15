//! Tests for `src/native/lark/analysis.rs`: the sizes, depths and growth
//! the generator steers by, over small hand-written grammars.

use super::*;
use crate::native::lark::builder::{NoImports, load_grammar};
use crate::native::lark::compile::compile;
use alloc::format;
use alloc::string::String;
use alloc::vec;

fn analyzed(text: &str) -> (Grammar, Analysis) {
    let built = load_grammar(text, &NoImports).unwrap();
    let grammar = compile(&built, &["start"]).unwrap();
    let analysis = analyze(&grammar, term_min(&grammar), term_typical(&grammar)).unwrap();
    (grammar, analysis)
}

fn term_min(grammar: &Grammar) -> Vec<u64> {
    grammar.terminals.iter().map(|t| t.min_width).collect()
}

fn term_typical(grammar: &Grammar) -> Vec<u64> {
    grammar.terminals.iter().map(|t| 3 * t.min_width).collect()
}

fn rule<'a>(grammar: &Grammar, analysis: &'a Analysis, name: &str) -> &'a Rule {
    &analysis.rules[grammar.rule_id(name).unwrap()]
}

#[test]
fn terminal_sizes_are_their_shortest_match() {
    let (g, a) = analyzed("start: \"ab\" NUM\nNUM: /[0-9]+/");
    let start = rule(&g, &a, "start");
    assert_eq!(start.min_size, 3);
    assert_eq!(start.min_depth, 1);
    let alt = &start.alternatives[0];
    assert_eq!(alt.max_size, Some(3));
    assert!(!alt.growing());
    assert_eq!(alt.items[0].min_size, 2);
    assert_eq!(
        alt.items[1].kind,
        NodeKind::Terminal(g.terminal_id("NUM").unwrap())
    );
}

#[test]
fn alternatives_sort_by_size_then_growth() {
    let (g, a) = analyzed("start: start \"x\" | \"a\" | \"bc\" | \"y\" start\n");
    let start = rule(&g, &a, "start");
    let sizes: Vec<(u64, bool)> = start
        .alternatives
        .iter()
        .map(|alt| (alt.min_size, alt.growing()))
        .collect();
    assert_eq!(sizes, vec![(1, false), (2, false), (2, true), (2, true)]);
    assert_eq!(start.min_size, 1);
}

#[test]
fn repeats_scale_the_minimum_and_bound_the_maximum() {
    let (g, a) = analyzed("start: \"a\"~2..3 \"b\"* \"c\"+ [\"d\"]\n");
    let start = rule(&g, &a, "start");
    assert_eq!(start.min_size, 3);
    let alt = &start.alternatives[0];
    assert!(alt.growing());
    let NodeKind::Repeat { min, max, inner } = &alt.items[0].kind else {
        panic!("expected a repeat");
    };
    assert_eq!((*min, *max), (2, Some(3)));
    assert_eq!(inner.min_size, 1);
    assert_eq!(alt.items[0].max_size, Some(3));
    assert_eq!(alt.items[1].min_size, 0);
    assert_eq!(alt.items[1].max_size, None);
    assert_eq!(alt.items[2].min_size, 1);
    assert_eq!(alt.items[3].max_size, Some(1));
    assert_eq!(alt.items[3].min_depth, 0);
}

#[test]
fn repeat_of_a_rule_counts_its_depth_only_when_required() {
    let (g, a) = analyzed("start: inner* | inner+\ninner: \"x\"\n");
    let start = rule(&g, &a, "start");
    assert_eq!(start.min_depth, 1);
    assert_eq!(start.alternatives[0].min_depth, 0);
    assert_eq!(start.alternatives[1].min_depth, 1);
    assert_eq!(start.alternatives[1].min_size, 1);
}

#[test]
fn groups_take_the_smallest_alternative() {
    let (g, a) = analyzed("start: (\"abc\" | \"d\" inner | \"ef\")\ninner: \"g\" \"h\"\n");
    let start = rule(&g, &a, "start");
    assert_eq!(start.min_size, 2);
    let NodeKind::Group(_, alts) = &start.alternatives[0].items[0].kind else {
        panic!("expected a group");
    };
    assert_eq!(alts.len(), 3);
    assert_eq!(alts[0].min_size, 2);
    assert_eq!(alts[2].min_size, 3);
    assert_eq!(start.alternatives[0].items[0].max_size, Some(3));
    assert_eq!(start.alternatives[0].items[0].min_depth, 0);
}

#[test]
fn recursion_that_adds_characters_is_unbounded() {
    let (g, a) = analyzed("start: a\na: \"x\" | \"(\" a \")\"\n");
    assert!(rule(&g, &a, "a").alternatives[1].growing());
    assert!(rule(&g, &a, "start").alternatives[0].growing());
    assert_eq!(rule(&g, &a, "start").min_depth, 2);
}

#[test]
fn recursion_that_adds_nothing_stays_bounded() {
    let (g, a) = analyzed("start: a\na: \"x\" | b\nb: a | \"yz\"\n");
    assert_eq!(rule(&g, &a, "a").alternatives[1].max_size, Some(2));
    assert!(!rule(&g, &a, "start").alternatives[0].growing());
    assert_eq!(rule(&g, &a, "start").alternatives[0].max_size, Some(2));
}

#[test]
fn unbounded_repeat_of_the_empty_group_is_bounded() {
    let (g, a) = analyzed("start: (\"a\"?)* \"b\"\n");
    assert_eq!(rule(&g, &a, "start").alternatives[0].max_size, None);
    let (g, a) = analyzed("start: (\"a\"~0)* \"b\"\n");
    assert_eq!(rule(&g, &a, "start").alternatives[0].max_size, Some(1));
}

#[test]
fn unproductive_alternatives_and_rules_are_dropped() {
    let (g, a) = analyzed(
        "start: \"a\" | loop | useless* | (loop | \"bb\")\nloop: \"x\" loop\nuseless: useless \"y\"\n",
    );
    let start = rule(&g, &a, "start");
    assert_eq!(start.alternatives.len(), 3);
    assert_eq!(start.alternatives[0].min_size, 0);
    let NodeKind::Repeat { min, max, .. } = &start.alternatives[0].items[0].kind else {
        panic!("expected a repeat");
    };
    assert_eq!((*min, *max), (0, Some(0)));
    assert_eq!(start.alternatives[0].max_size, Some(0));
    let NodeKind::Group(_, alts) = &start.alternatives[2].items[0].kind else {
        panic!("expected a group");
    };
    assert_eq!(alts.len(), 1);
    assert_eq!(alts[0].min_size, 2);
    let unproductive = rule(&g, &a, "loop");
    assert_eq!(unproductive.min_size, 0);
    assert_eq!(unproductive.min_depth, 0);
    assert!(unproductive.alternatives.is_empty());
}

#[test]
fn typical_sizes_count_terminals_at_their_typical_length() {
    let (g, a) =
        analyzed("start: \"ab\" NUM | start NUM | (NUM | \"ab\" \"ab\") NUM~2 NUM+\nNUM: /[0-9]+/");
    let start = rule(&g, &a, "start");
    assert_eq!(start.min_size, 3);
    assert_eq!(start.typical_size, 9);
    let alts = &start.alternatives;
    assert_eq!(alts[0].typical_size, 9);
    assert_eq!(alts[0].items[0].typical_size, 6);
    assert_eq!(alts[0].items[1].typical_size, 3);
    assert_eq!(alts[1].typical_size, 12);
    assert_eq!(alts[2].min_size, 4);
    assert_eq!(alts[2].typical_size, 12);
    assert_eq!(alts[2].items[0].typical_size, 3);
    assert_eq!(alts[2].items[1].typical_size, 6);
    assert_eq!(alts[2].items[2].typical_size, 3);
}

#[test]
fn unproductive_start_rule_is_an_error() {
    let built = load_grammar("start: start \"x\" | \"y\" start\n", &NoImports).unwrap();
    let grammar = compile(&built, &["start"]).unwrap();
    let err = analyze(&grammar, term_min(&grammar), term_typical(&grammar)).unwrap_err();
    assert_eq!(
        err.message,
        "Rule 'start' can never produce a finite string: every alternative recurses \
         without a base case"
    );
}

#[test]
fn item_max_size_of_unbounded_repeat_inside_a_fixed_repeat() {
    let (g, a) = analyzed("start: (\"a\"*)~2\n");
    let alt = &rule(&g, &a, "start").alternatives[0];
    assert_eq!(alt.max_size, None);
    assert_eq!(alt.min_size, 0);
}

#[test]
fn bounds_saturate_instead_of_overflowing() {
    assert_eq!(add_bound(Some(u64::MAX), Some(1)), Some(u64::MAX));
    assert_eq!(add_bound(None, Some(1)), None);
    assert_eq!(max_bound(Some(3), Some(5)), Some(5));
    assert_eq!(max_bound(Some(3), None), None);
}

#[test]
fn rules_are_visited_after_the_rules_they_use() {
    let (g, _) = analyzed("start: a b\na: b | a \"x\"\nb: c\nc: \"c\"\n");
    let order = dependency_order(&g);
    let position = |name: &str| {
        order
            .iter()
            .position(|&r| r == g.rule_id(name).unwrap())
            .unwrap()
    };
    assert_eq!(order.len(), 4);
    assert!(position("c") < position("b"));
    assert!(position("b") < position("a"));
    assert!(position("a") < position("start"));
}

#[test]
fn a_long_chain_defined_from_the_top_down_is_analysed_in_one_round() {
    let mut text = String::from("start: r0\n");
    for i in 0..3000 {
        text.push_str(&format!("r{i}: r{}\n", i + 1));
    }
    text.push_str("r3000: \"a\"\n");
    let (g, a) = analyzed(&text);
    assert_eq!(
        dependency_order(&g).last(),
        Some(&g.rule_id("start").unwrap())
    );
    let start = rule(&g, &a, "start");
    assert_eq!(start.min_size, 1);
    assert_eq!(start.min_depth, 3002);
    assert_eq!(start.alternatives[0].max_size, Some(1));
}
