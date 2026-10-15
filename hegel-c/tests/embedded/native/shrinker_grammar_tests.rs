//! Tests for `src/native/shrinker/grammar.rs`: the grammar-directed pass
//! on strings under a grammar.

use super::{Derived, LeafStage};
use crate::exchange::drive_no_yield;
use crate::native::core::Constraint;
use crate::native::core::{ChoiceNode, ChoiceValue, Spans, StringChoice};
use crate::native::draws::StringSpec;
use crate::native::draws::text::TextAlphabet;
use crate::native::intervalsets::IntervalSet;
use crate::native::lark::builder::{NoImports, load_grammar};
use crate::native::lark::compile::{compile, placeholder};
use crate::native::lark::constraint::GrammarConstraint;
use crate::native::lark::generate::{GrammarOptions, from_lark};
use crate::native::lark::layout::Layout;
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

fn cps(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

fn text_of(value: &ChoiceValue) -> String {
    match value {
        ChoiceValue::String(v) => v.iter().map(|&cp| char::from_u32(cp).unwrap()).collect(),
        _ => unreachable!(),
    }
}

fn first_text(nodes: &[ChoiceNode]) -> Option<String> {
    match nodes.first()?.value() {
        ChoiceValue::String(v) => Some(v.iter().map(|&cp| char::from_u32(cp).unwrap()).collect()),
        _ => None,
    }
}

fn grammar_node(grammar_text: &str, simplest: &str, value: &str, forced: bool) -> ChoiceNode {
    let built = load_grammar(grammar_text, &NoImports).unwrap();
    let grammar = compile(&built, &["start"]).unwrap();
    let start = grammar.start[0];
    let constraint = GrammarConstraint::new(grammar, start, 64, u32::MAX, None).unwrap();
    constraint
        .simplest_or_init(|| Ok::<_, crate::control::InternalError>(cps(simplest)))
        .unwrap();
    ChoiceNode::string(
        StringChoice {
            intervals: Arc::clone(constraint.intervals()),
            min_size: 0,
            max_size: 64,
            constraint: Some(Constraint::Grammar(Arc::new(constraint))),
        },
        cps(value),
        forced,
    )
}

fn shrinker_where(
    nodes: Vec<ChoiceNode>,
    interesting: impl Fn(&str) -> bool + Send + 'static,
) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => (
                first_text(nodes).is_some_and(|s| interesting(&s)),
                nodes.to_vec(),
                Spans::new(),
            ),
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        nodes,
        Spans::new(),
    )
}

const INT_LIST: &str = "start: INT (\",\" INT)*\nINT: /[0-9]+/\n";
const NESTED: &str = "start: \"[\" (start (\",\" start)*)? \"]\" | \"x\"\n";

#[test]
fn an_int_list_shrinks_to_two_zeros() {
    let node = grammar_node(INT_LIST, "0", "12,3,45", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.contains(','));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "0,0");
}

#[test]
fn the_whole_shrink_reaches_the_same_answer() {
    let node = grammar_node(INT_LIST, "0", "12,3,45", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.contains(','));
    drive_no_yield(shrinker.shrink()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "0,0");
}

#[test]
fn every_repetition_is_deleted_at_once_when_the_result_is_still_interesting() {
    let node = grammar_node(NESTED, "x", "[x,x,x]", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.starts_with('['));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "[]");
}

/// X's text is deleted outright, since the other alternative accepts
/// the text without it; Y's cannot be.
#[test]
fn a_leaf_is_deleted_when_the_grammar_still_accepts_the_text() {
    let node = grammar_node(
        "start: \"a\" X Y \"c\" | \"a\" Y \"c\"\nX: /x+/\nY: /y+/\n",
        "axyc",
        "axyc",
        false,
    );
    let mut shrinker = shrinker_where(vec![node], |s| s.contains('y'));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "ayc");
}

/// No edit of a string literal's text is in the grammar, so none is
/// tried: every candidate keeps the literal whole.
#[test]
fn a_leaf_of_a_literal_is_left_as_it_is() {
    let node = grammar_node("start: \"key\" /[a-z]+/\n", "keya", "keyzz", false);
    let seen = Arc::new(crate::sys::sync::Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let mut shrinker = shrinker_where(vec![node], move |s| {
        log.lock().push(String::from(s));
        true
    });
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "keya");
    let seen: Vec<String> = seen.lock().clone();
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|s| s.starts_with("key")), "{seen:?}");
}

#[test]
fn a_rule_is_replaced_by_a_smaller_node_of_the_same_rule_inside_it() {
    let node = grammar_node(NESTED, "x", "[x,[x,[x,x]]]", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.contains("[x,x]"));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "[x,x]");
}

#[test]
fn repetitions_are_deleted_in_windows() {
    let node = grammar_node("start: \"a\"*\n", "", &"a".repeat(23), false);
    let mut shrinker = shrinker_where(vec![node], |s| s.len() >= 10);
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "a".repeat(10));
}

/// Deleting every "b" takes the other alternative of `bs`, which has no
/// repeat; the repeat after the one that went is looked at all the same.
#[test]
fn a_repeat_that_goes_with_its_repetitions_does_not_hide_the_next() {
    let node = grammar_node(
        "start: bs \"c\"* \"d\"\nbs: \"b\"+ |\n",
        "d",
        "bbcccd",
        false,
    );
    let mut shrinker = shrinker_where(vec![node], |_| true);
    assert!(matches!(
        drive_no_yield(shrinker.delete_repetitions(0)),
        Ok(true)
    ));
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "d");
}

#[test]
fn ignored_text_is_cut_and_what_must_stay_is_shortened() {
    let grammar = "start: \"a\" \"b\" \"c\"\n%ignore WS\nWS: / +/\n";
    let node = grammar_node(grammar, "abc", "a   b  c", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.len() >= 5);
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "a  bc");
    let node = grammar_node(grammar, "abc", "a   b  c", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.contains("a "));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "a bc");
}

#[test]
fn a_leaf_is_shortened_from_the_back_down_to_its_first_character() {
    let node = grammar_node("start: /x?[a-z]/\n", "a", "xb", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.contains('b'));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "b");
}

#[test]
fn a_leaf_is_lowered_character_by_character() {
    let node = grammar_node("start: /[a-z]+/\n", "a", "zqz", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.len() == 3);
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "aaa");
}

#[test]
fn a_rule_is_replaced_by_the_smallest_text_seen_for_it() {
    let node = grammar_node(NESTED, "x", "[[x,x],[x,x]]", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.starts_with("[[x,x],"));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "[[x,x],x]");
}

#[test]
fn a_string_the_grammar_does_not_derive_is_left_alone() {
    let node = grammar_node("start: \"a\"\n", "a", "zzz", false);
    let mut shrinker = shrinker_where(vec![node], |_| false);
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "zzz");
}

#[test]
fn a_forced_string_and_a_string_without_a_grammar_are_left_alone() {
    let forced = grammar_node(INT_LIST, "0", "12,3", true);
    let plain = ChoiceNode::string(
        StringChoice {
            intervals: Arc::new(
                crate::native::intervalsets::IntervalSet::new(vec![(b'a' as u32, b'z' as u32)])
                    .unwrap(),
            ),
            min_size: 0,
            max_size: 8,
            constraint: None,
        },
        cps("zz"),
        false,
    );
    let mut shrinker = shrinker_where(vec![forced, plain], |_| true);
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "12,3");
    assert_eq!(text_of(&shrinker.current_nodes[1].value()), "zz");
}

/// The ignored text either side of a placeholder is shortened but
/// never cut, and the rest of the ignored text goes.
#[test]
fn ignored_text_beside_a_placeholder_stays() {
    let spec = from_lark(
        "start: \"a\" X \"b\" \"c\"\n%declare X\nWS: / +/\n%ignore WS\n",
        &NoImports,
        GrammarOptions {
            explicit: vec!["X".to_string()],
            ..GrammarOptions::default()
        },
    )
    .unwrap();
    let constraint = Arc::clone(spec.constraint());
    let marker = placeholder("X");
    let simplest = format!("a {marker} bc");
    constraint
        .simplest_or_init(|| Ok::<_, crate::control::InternalError>(cps(&simplest)))
        .unwrap();
    let node = ChoiceNode::string(
        StringChoice {
            intervals: Arc::clone(constraint.intervals()),
            min_size: 0,
            max_size: 64,
            constraint: Some(Constraint::Grammar(constraint)),
        },
        cps(&format!("a   {marker}   b   c")),
        false,
    );
    let mut shrinker = shrinker_where(vec![node], |s| s.ends_with("bc") || s.ends_with("c"));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), simplest);
}

fn derived(grammar_text: &str, text: &str) -> Derived {
    let built = load_grammar(grammar_text, &NoImports).unwrap();
    let grammar = compile(&built, &["start"]).unwrap();
    let start = grammar.start[0];
    let lower = IntervalSet::new(vec![(u32::from('a'), u32::from('z'))]).unwrap();
    let constraint =
        Arc::new(GrammarConstraint::new(grammar, start, 64, u32::MAX, Some(&lower)).unwrap());
    let kind = StringChoice {
        intervals: Arc::clone(constraint.intervals()),
        min_size: 0,
        max_size: 64,
        constraint: Some(Constraint::Grammar(Arc::clone(&constraint))),
    };
    let text = cps(text);
    let layout = Layout::new(&constraint.tree(&text).unwrap());
    Derived {
        kind,
        constraint,
        text,
        layout,
    }
}

/// After an accepted lowering the text is derived again, and the leaf
/// may be another: the search goes on from the character accepted when
/// the leaf still has it, and starts over from whatever it has instead.
#[test]
fn lowering_starts_over_when_the_character_under_it_has_changed() {
    let d = derived("start: /[a-z]+/\n", "zz");
    let mut stage = LeafStage::Lower { p: 0, search: None };
    let candidate = |stage: &mut LeafStage, d: &Derived, start, end| {
        stage.candidate(d, start, end).ok().unwrap()
    };
    assert_eq!(candidate(&mut stage, &d, 0, 2), Some(cps("az")));
    stage.record(true);
    let same = derived("start: /[a-z]+/\n", "az");
    assert_eq!(candidate(&mut stage, &same, 0, 2), Some(cps("aa")));
    let mut stage = LeafStage::Lower { p: 0, search: None };
    candidate(&mut stage, &d, 0, 2).unwrap();
    stage.record(true);
    let moved = derived("start: /[a-z]+/\n", "zc");
    assert_eq!(candidate(&mut stage, &moved, 1, 2), Some(cps("za")));
}

#[test]
fn an_ambiguous_grammar_shrinks_through_its_changing_derivations() {
    let node = grammar_node("start: A+\nA: /x+/\n", "x", "xxxxxxx", false);
    let mut shrinker = shrinker_where(vec![node], |s| s.len() >= 2);
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(text_of(&shrinker.current_nodes[0].value()), "xx");
}

#[test]
fn a_string_under_an_alphabet_with_a_placeholder_shrinks_within_both() {
    let abc = StringSpec::text(
        &TextAlphabet {
            min_codepoint: u32::from('a'),
            max_codepoint: u32::from('c'),
            include_characters: Some(" ".to_string()),
            ..TextAlphabet::default()
        },
        0,
        8,
    )
    .unwrap();
    let spec = from_lark(
        "start: WORD X WORD\nWORD: /[a-z]+/\n%declare X\nWS: / +/\n%ignore WS\n",
        &NoImports,
        GrammarOptions {
            explicit: vec!["X".to_string()],
            alphabet: Some(Arc::new(abc)),
            ..GrammarOptions::default()
        },
    )
    .unwrap();
    let constraint = Arc::clone(spec.constraint());
    let marker = placeholder("X");
    let simplest = format!("a {marker} a");
    constraint
        .simplest_or_init(|| Ok::<_, crate::control::InternalError>(cps(&simplest)))
        .unwrap();
    let node = ChoiceNode::string(
        StringChoice {
            intervals: Arc::clone(constraint.intervals()),
            min_size: 0,
            max_size: usize::MAX,
            constraint: Some(Constraint::Grammar(constraint)),
        },
        cps(&format!("cba  {marker}  bcc")),
        false,
    );
    let mut shrinker = shrinker_where(vec![node], |s| s.contains('c'));
    drive_no_yield(shrinker.shrink_grammar_strings()).unwrap();
    assert_eq!(
        text_of(&shrinker.current_nodes[0].value()),
        format!("a {marker} c")
    );
}
