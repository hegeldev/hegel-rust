//! Tests for `src/native/lark/layout.rs`: the ranges a derivation's
//! nodes cover, the structural questions answered over them, and the
//! coarse candidates built from them.

use super::*;
use crate::native::lark::builder::{NoImports, load_grammar};
use crate::native::lark::compile::{Grammar, compile};
use crate::native::lark::constraint::GrammarConstraint;
use alloc::string::String;
use alloc::vec;

fn cps(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

fn text_of(text: &[u32], range: (usize, usize)) -> String {
    text[range.0..range.1]
        .iter()
        .map(|&cp| char::from_u32(cp).unwrap())
        .collect()
}

fn laid_out(grammar_text: &str, text: &str) -> (Grammar, GrammarConstraint, Layout, Vec<u32>) {
    let built = load_grammar(grammar_text, &NoImports).unwrap();
    let grammar = compile(&built, &["start"]).unwrap();
    let start = grammar.start[0];
    let constraint = GrammarConstraint::new(grammar.clone(), start, 64, u32::MAX, None).unwrap();
    let text = cps(text);
    let tree = constraint.tree(&text).unwrap();
    (grammar, constraint, Layout::new(&tree), text)
}

const NESTED: &str = "start: \"[\" (start (\",\" start)*)? \"]\" | \"x\"\n";

#[test]
fn depth_counts_the_rules_nested_on_the_deepest_path() {
    for (text, depth) in [
        ("x", 1),
        ("[]", 1),
        ("[x]", 2),
        ("[x,[x,x]]", 3),
        ("[[[x]],x]", 4),
    ] {
        let (_, _, layout, _) = laid_out(NESTED, text);
        assert_eq!(layout.depth(), depth, "{text}");
    }
    for (text, depth) in [("x", 1), ("(x)", 2), ("(((x)))", 4)] {
        let (_, _, layout, _) = laid_out("start: \"(\" start \")\" | \"x\"\n", text);
        assert_eq!(layout.depth(), depth, "{text}");
    }
}

#[test]
fn nodes_cover_their_children_contiguously_and_leaves_cover_the_text() {
    let (_, _, layout, text) = laid_out(NESTED, "[x,[x,x],[]]");
    let nodes = layout.nodes();
    assert_eq!((nodes[0].start, nodes[0].end), (0, text.len()));
    for node in nodes {
        let mut at = node.start;
        for &child in &node.children {
            assert_eq!(nodes[child].start, at);
            at = nodes[child].end;
        }
        if !node.children.is_empty() {
            assert_eq!(at, node.end);
        }
    }
    leaves_cover(&layout, &text);
}

fn leaves_cover(layout: &Layout, text: &[u32]) {
    let mut covered = Vec::new();
    for (_, start, end) in layout.leaves() {
        assert_eq!(start, covered.len());
        covered.extend_from_slice(&text[start..end]);
    }
    assert_eq!(covered, text);
}

#[test]
fn ignored_text_at_the_edges_of_a_node_is_outside_its_range() {
    let grammar = "start: item (\",\" item)*\nitem: INT | \"[\" start \"]\"\nINT: /[0-9]+/\n%ignore WS\nWS: / +/\n";
    let (grammar, _, layout, text) = laid_out(grammar, " [ 7 , 3 ] ");
    leaves_cover(&layout, &text);
    let item = grammar.rule_id("item").unwrap();
    let shown: Vec<(String, String)> = layout
        .rules()
        .iter()
        .map(|&(id, s, e)| (grammar.rules[id].name.clone(), text_of(&text, (s, e))))
        .collect();
    let rule = |name: &str, s: &str| (String::from(name), String::from(s));
    assert_eq!(
        shown,
        vec![
            rule("start", "[ 7 , 3 ]"),
            rule("item", "[ 7 , 3 ]"),
            rule("start", "7 , 3"),
            rule("item", "7"),
            rule("item", "3"),
        ]
    );
    assert_eq!(layout.rules()[3].0, item);
    assert_eq!(
        layout.ignored(),
        vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9), (10, 11)]
    );
    let ranges: Vec<String> = layout.repeats()[0]
        .iter()
        .map(|&range| text_of(&text, range))
        .collect();
    assert_eq!(ranges, vec![", 3"]);
}

#[test]
fn a_node_deriving_no_text_sits_where_it_begins() {
    let (_, _, layout, text) = laid_out("start: \"a\"* \"b\"\n%ignore WS\nWS: / +/\n", " b");
    leaves_cover(&layout, &text);
    let empty: Vec<&Node> = layout
        .nodes()
        .iter()
        .filter(|n| n.part == Part::Repeat)
        .collect();
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0].start, empty[0].end);
    assert!(layout.repeats()[0].is_empty());
    assert_eq!((layout.nodes()[0].start, layout.nodes()[0].end), (1, 2));
}

#[test]
fn leaves_cover_the_text_of_a_nested_derivation() {
    let (_, _, layout, text) = laid_out(NESTED, "[x,[x,x],[]]");
    leaves_cover(&layout, &text);
    assert!(
        layout
            .nodes()
            .iter()
            .any(|n| n.part == Part::Repeat && n.start == n.end)
    );
}

#[test]
fn hoists_pair_each_rule_node_with_the_smaller_same_rule_nodes_inside_it() {
    let (_, _, layout, text) = laid_out(NESTED, "[x,[x]]");
    let shown: Vec<(String, String)> = layout
        .hoists()
        .into_iter()
        .map(|(a, d)| {
            let (a, d) = (&layout.nodes()[a], &layout.nodes()[d]);
            (
                text_of(&text, (a.start, a.end)),
                text_of(&text, (d.start, d.end)),
            )
        })
        .collect();
    let pair = |a: &str, d: &str| (String::from(a), String::from(d));
    assert_eq!(
        shown,
        vec![
            pair("[x,[x]]", "x"),
            pair("[x,[x]]", "x"),
            pair("[x,[x]]", "[x]"),
            pair("[x]", "x"),
        ]
    );
    let (_, _, flat, _) = laid_out("start: \"a\" b*\nb: \"b\"\n", "abb");
    assert!(flat.hoists().is_empty());
}

#[test]
fn hoists_skip_the_same_rule_nodes_at_distances_that_are_no_power_of_two() {
    let (_, _, layout, text) = laid_out(NESTED, "[[[[x]]]]");
    let shown: Vec<(String, String)> = layout
        .hoists()
        .into_iter()
        .map(|(a, d)| {
            let (a, d) = (&layout.nodes()[a], &layout.nodes()[d]);
            (
                text_of(&text, (a.start, a.end)),
                text_of(&text, (d.start, d.end)),
            )
        })
        .collect();
    let pair = |a: &str, d: &str| (String::from(a), String::from(d));
    assert_eq!(
        shown,
        vec![
            pair("[[[[x]]]]", "x"),
            pair("[[[[x]]]]", "[[x]]"),
            pair("[[[[x]]]]", "[[[x]]]"),
            pair("[[[x]]]", "[x]"),
            pair("[[[x]]]", "[[x]]"),
            pair("[[x]]", "x"),
            pair("[[x]]", "[x]"),
            pair("[x]", "x"),
        ]
    );
}

#[test]
fn repeats_list_the_repetitions_of_each_repeat_without_the_ignored_text() {
    let (_, _, layout, text) = laid_out("start: \"a\" (\"b\" | \"cc\")*\n", "abccb");
    assert_eq!(layout.repeats(), vec![vec![(1, 2), (2, 4), (4, 5)]]);
    let (_, _, spaced, text2) = laid_out(
        "start: INT (\",\" INT)*\nINT: /[0-9]+/\n%ignore \" \"\n",
        "1 , 2 ,3",
    );
    let repeats = spaced.repeats();
    assert_eq!(repeats.len(), 1);
    let items: Vec<String> = repeats[0]
        .iter()
        .map(|&range| text_of(&text2, range))
        .collect();
    assert_eq!(items, vec![", 2", ",3"]);
    assert_eq!(text_of(&text, (0, 1)), "a");
}

#[test]
fn ignored_ranges_and_leaves_are_in_text_order() {
    let (grammar, _, layout, text) =
        laid_out("start: \"a\" \"b\"\n%ignore WS\nWS: / +/\n", "a   b");
    assert_eq!(layout.ignored(), vec![(1, 4)]);
    let leaves = layout.leaves();
    assert_eq!(
        leaves.iter().map(|&(_, s, e)| (s, e)).collect::<Vec<_>>(),
        vec![(0, 1), (1, 4), (4, 5)]
    );
    assert_eq!(leaves[1].0, grammar.ignore[0]);
    assert_eq!(leaves[0].0, grammar.terminal_id("A").unwrap());
    assert_eq!(text_of(&text, (1, 4)), "   ");
}

#[test]
fn rules_are_listed_in_pre_order() {
    let (grammar, _, layout, text) = laid_out(NESTED, "[x,[x]]");
    let shown: Vec<String> = layout
        .rules()
        .iter()
        .map(|&(id, s, e)| {
            assert_eq!(grammar.rules[id].name, "start");
            text_of(&text, (s, e))
        })
        .collect();
    assert_eq!(shown, vec!["[x,[x]]", "x", "[x]", "x"]);
}

#[test]
fn coarse_candidates_are_each_move_once_and_never_the_text_itself() {
    let (grammar, constraint, layout, text) = laid_out(NESTED, "[x,[x]]");
    let x = grammar.terminal_id("X").unwrap();
    let candidates: Vec<String> = layout
        .coarse_candidates(&text, |id| (id == x).then(|| cps("y")))
        .map(|c| text_of(&c, (0, c.len())))
        .collect();
    for candidate in &candidates {
        assert_ne!(candidate, "[x,[x]]");
        assert_eq!(candidates.iter().filter(|c| *c == candidate).count(), 1);
    }
    for expected in ["[x]", "[]", "x", "[x,x]", "[y,[x]]", "[x,[y]]"] {
        assert!(
            candidates.contains(&String::from(expected)),
            "{candidates:?}"
        );
    }
    assert!(!constraint.accepts(&cps("[y]")));
}

#[test]
fn coarse_candidates_cut_the_ignored_text_and_shorten_repeats() {
    let (_, _, layout, text) = laid_out(
        "start: INT (\",\" INT)*\nINT: /[0-9]+/\n%ignore \" \"\n",
        "1, 2, 3, 4, 5",
    );
    let candidates: Vec<String> = layout
        .coarse_candidates(&text, |_| None)
        .map(|c| text_of(&c, (0, c.len())))
        .collect();
    assert_eq!(candidates, vec!["1,2,3,4,5", "1", "1, 2, 3", "1, 2, 3, 4"]);
}

#[test]
fn splice_and_cut_edit_the_text() {
    let text = cps("abcdef");
    assert_eq!(splice(&text, 2, 4, &cps("XY Z")), cps("abXY Zef"));
    assert_eq!(splice(&text, 0, 0, &[]), text);
    assert_eq!(splice(&text, 6, 6, &cps("!")), cps("abcdef!"));
    assert_eq!(cut(&text, &[(0, 1), (2, 3), (5, 6)]), cps("bde"));
    assert_eq!(cut(&text, &[]), text);
}
