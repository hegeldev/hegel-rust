//! Tests for `src/native/lark/mutate.rs`: the moves a derivation offers
//! and the texts they produce.

use super::*;
use crate::native::HashSet;
use crate::native::lark::builder::{NoImports, load_grammar};
use crate::native::lark::compile::compile;
use crate::native::lark::constraint::GrammarConstraint;
use alloc::string::String;

fn cps(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

fn text_of(text: &[u32]) -> String {
    text.iter().map(|&cp| char::from_u32(cp).unwrap()).collect()
}

fn laid_out(grammar_text: &str, text: &str) -> (GrammarConstraint, Layout, Vec<u32>) {
    let built = load_grammar(grammar_text, &NoImports).unwrap();
    let grammar = compile(&built, &["start"]).unwrap();
    let start = grammar.start[0];
    let constraint = GrammarConstraint::new(grammar, start, 64, u32::MAX, None).unwrap();
    let text = cps(text);
    let tree = constraint.tree(&text).unwrap();
    (constraint, Layout::new(&tree), text)
}

const NESTED: &str = "start: \"[\" (start (\",\" start)*)? \"]\" | \"x\"\n";
const WORDS: &str = "start: WORD+\nWORD: /[a-z]+/\n%ignore WS\nWS: / +/\n";

#[test]
fn every_move_on_a_nested_list_is_a_copy_between_start_nodes_or_a_duplicated_item() {
    let (_, layout, text) = laid_out(NESTED, "[x,[x]]");
    let all = Moves::of(&layout);
    assert_eq!(all.len(), 4 * 3 + 3);
    let moves: Vec<Move> = (0..all.len()).map(|k| all.get(k)).collect();
    let distinct: HashSet<Move> = moves.iter().copied().collect();
    assert_eq!(distinct.len(), moves.len());
    let texts: HashSet<String> = moves
        .into_iter()
        .map(|mv| text_of(&apply(&layout, &text, mv)))
        .collect();
    for expected in ["[x,[x],[x]]", "[x,[xx]]", "x", "[x,[x,[x]]]", "[[x],[x]]"] {
        assert!(
            texts.contains(expected),
            "{expected} missing from {texts:?}"
        );
    }
}

#[test]
fn a_duplicated_repetition_keeps_the_text_separating_it_from_its_neighbour() {
    let (constraint, layout, text) = laid_out(WORDS, "ab  cd e");
    let all = Moves::of(&layout);
    let duplicates: Vec<String> = (0..all.len())
        .map(|k| all.get(k))
        .filter(|mv| matches!(mv, Move::Duplicate { .. }))
        .map(|mv| text_of(&apply(&layout, &text, mv)))
        .collect();
    assert_eq!(duplicates, ["ab  ab  cd e", "ab  cd  cd e", "ab  cd e e"]);
    for grown in &duplicates {
        assert!(constraint.accepts(&cps(grown)), "{grown:?}");
    }
}

#[test]
fn a_lone_repetition_is_duplicated_as_it_is() {
    let (_, layout, text) = laid_out(WORDS, "ab");
    let all = Moves::of(&layout);
    let grown: Vec<String> = (0..all.len())
        .map(|k| all.get(k))
        .map(|mv| text_of(&apply(&layout, &text, mv)))
        .collect();
    assert_eq!(grown, ["abab"]);
}

#[test]
fn growing_proposes_a_changed_text_or_nothing() {
    let (constraint, layout, text) = laid_out(NESTED, "[x,[x]]");
    let mut rng = EngineRng::seeded(3);
    let mut seen = HashSet::default();
    let mut none = 0;
    for _ in 0..200 {
        match grow(&layout, &text, &mut rng) {
            Some(grown) => {
                assert_ne!(grown, text);
                seen.insert(text_of(&grown));
            }
            None => none += 1,
        }
    }
    assert!(none > 0, "copying one x over the other changes nothing");
    assert!(seen.len() >= 5, "{seen:?}");
    assert!(seen.iter().any(|s| constraint.accepts(&cps(s))));
    assert!(
        seen.iter().any(|s| !constraint.accepts(&cps(s))),
        "{seen:?}"
    );

    let (_, layout, text) = laid_out("start: \"x\"\n", "x");
    assert_eq!(grow(&layout, &text, &mut rng), None);
}
