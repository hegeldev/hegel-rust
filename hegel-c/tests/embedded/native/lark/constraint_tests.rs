//! Tests for `src/native/lark/constraint.rs`: acceptance through the
//! parser, the derivation memo and its capacity, and identity across
//! compilations.

use super::*;
use crate::control::InternalError;
use crate::native::lark::builder::{NoImports, load_grammar};
use crate::native::lark::compile::{compile, placeholder};
use crate::native::lark::generate::{GrammarOptions, from_lark};
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use core::hash::BuildHasher;

fn compiled(text: &str, start: &str) -> Grammar {
    let built = load_grammar(text, &NoImports).unwrap();
    compile(&built, &[start]).unwrap()
}

fn constraint(text: &str, start: &str, max_size: usize, simplest: &str) -> GrammarConstraint {
    let grammar = compiled(text, start);
    let start = grammar.start[0];
    let c = GrammarConstraint::new(grammar, start, max_size, u32::MAX, None).unwrap();
    c.simplest_or_init(|| Ok::<_, InternalError>(cps(simplest)))
        .unwrap();
    c
}

fn cps(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

const AB: &str = "start: \"a\" \"b\"*\n";

#[test]
fn size_limits_exclude_both_needed_and_ignored_placeholders() {
    let plain = constraint(AB, "start", 2, "a");
    assert!(plain.accepts(&cps("ab")));
    assert!(!plain.accepts(&cps("abb")));
    for text in [
        "start: X /a+/\n%declare X\n",
        "start: /a+/\n%declare X\n%ignore X\n",
    ] {
        let spec = from_lark(
            text,
            &NoImports,
            GrammarOptions {
                explicit: vec!["X".to_string()],
                max_size: 2,
                ..GrammarOptions::default()
            },
        )
        .unwrap();
        let c = spec.constraint();
        let marker = placeholder("X");
        assert!(c.accepts(&cps(&format!("{marker}aa"))));
        assert!(!c.accepts(&cps(&format!("{marker}aaa"))));
    }
}

#[test]
fn accepts_the_strings_the_grammar_derives_and_rejects_the_rest() {
    let c = constraint(AB, "start", 16, "a");
    assert!(c.accepts(&cps("a")));
    assert!(c.accepts(&cps("ab")));
    assert!(c.accepts(&cps("abbb")));
    assert!(!c.accepts(&cps("")));
    assert!(!c.accepts(&cps("b")));
    assert!(!c.accepts(&cps("ba")));
    assert!(!c.accepts(&cps("abc")));
    assert_eq!(c.simplest(), Some(cps("a").as_slice()));
    assert_eq!(c.max_size(), 16);
    assert_eq!(c.grammar().rules[c.start()].name, "start");
}

#[test]
fn a_codepoint_that_is_no_scalar_value_is_in_no_grammar() {
    let c = constraint("start: /./*\n", "start", 16, "");
    assert!(c.accepts(&cps("xyz")));
    assert!(!c.accepts(&[b'x' as u32, 0xD800]));
    assert_eq!(c.known_len(), 2);
}

#[test]
fn a_string_known_to_derive_is_not_parsed_again() {
    let c = constraint(AB, "start", 16, "a");
    let first = c.tree(&cps("abb")).unwrap();
    let again = c.tree(&cps("abb")).unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    assert_eq!(c.known_len(), 2);
    assert!(c.accepts(&cps("abb")));
    assert_eq!(c.known_len(), 2);
    assert!(!c.accepts(&cps("bb")));
    assert_eq!(c.known_len(), 2);
}

#[test]
fn the_memo_is_emptied_when_it_reaches_its_capacity() {
    let c = constraint("start: /[a-z]+/\n", "start", 16, "a");
    assert_eq!(c.known_len(), 1);
    let word = |i: usize| [b'a' as u32 + (i / 26) as u32, b'a' as u32 + (i % 26) as u32];
    for i in 0..KNOWN_CAPACITY - 1 {
        assert!(c.accepts(&word(i)));
    }
    assert_eq!(c.known_len(), KNOWN_CAPACITY);
    assert!(c.accepts(&word(KNOWN_CAPACITY)));
    assert_eq!(c.known_len(), 1);
    assert!(c.accepts(&word(0)));
    assert_eq!(c.known_len(), 2);
}

const LIST: &str =
    "start: item (\",\" item)*\nitem: INT | \"[\" start \"]\"\nINT: /[0-9]+/\n%ignore \" \"\n";

#[test]
fn the_smallest_text_each_rule_and_terminal_derives_is_learned() {
    let grammar = compiled(LIST, "start");
    let start = grammar.start[0];
    let item = grammar.rule_id("item").unwrap();
    let int = grammar.terminal_id("INT").unwrap();
    let space = grammar.ignore[0];
    let c = GrammarConstraint::new(grammar, start, 64, u32::MAX, None).unwrap();
    assert_eq!(c.smallest_for_rule(start), None);
    assert_eq!(c.smallest_for_terminal(int), None);
    assert!(c.accepts(&cps("12,  [7, 3]")));
    assert_eq!(c.smallest_for_rule(start), Some(cps("7, 3")));
    assert_eq!(c.smallest_for_rule(item), Some(cps("3")));
    assert_eq!(c.smallest_for_terminal(int), Some(cps("3")));
    assert_eq!(c.smallest_for_terminal(space), Some(cps(" ")));
    assert!(c.accepts(&cps("[9]")));
    assert_eq!(c.smallest_for_rule(start), Some(cps("9")));
    assert_eq!(c.smallest_for_rule(item), Some(cps("3")));
    assert!(c.accepts(&cps("0")));
    assert_eq!(c.smallest_for_rule(start), Some(cps("0")));
    assert_eq!(c.smallest_for_rule(item), Some(cps("0")));
    assert_eq!(c.smallest_for_terminal(int), Some(cps("0")));
    assert!(c.accepts(&cps("0")));
    assert_eq!(c.known_len(), 3);
}

#[test]
fn the_simplest_string_teaches_its_derivation() {
    let grammar = compiled(LIST, "start");
    let start = grammar.start[0];
    let int = grammar.terminal_id("INT").unwrap();
    let c = GrammarConstraint::new(grammar, start, 64, u32::MAX, None).unwrap();
    c.simplest_or_init(|| Ok::<_, u8>(cps("0"))).unwrap();
    assert_eq!(c.smallest_for_terminal(int), Some(cps("0")));
    assert_eq!(c.known_len(), 1);
}

#[test]
fn smaller_orders_by_length_then_by_the_shrink_order_of_characters() {
    let c = constraint(AB, "start", 16, "a");
    assert!(c.smaller(&cps("zz"), &cps("000")));
    assert!(c.smaller(&cps("0"), &cps("a")));
    assert!(c.smaller(&cps("Z"), &cps(" ")));
    assert!(c.smaller(&cps(" "), &cps("a")));
    assert!(c.smaller(&cps("a0"), &cps("aa")));
    assert!(!c.smaller(&cps("a"), &cps("a")));
    assert!(!c.smaller(&cps("aa"), &cps("a0")));
    assert_eq!(c.intervals().len(), 0x110000 - 0x800);
}

#[test]
fn constraints_compiled_from_the_same_grammar_are_the_same_kind() {
    let a = constraint(AB, "start", 16, "a");
    let b = constraint(AB, "start", 16, "a");
    assert_eq!(a, b);
    assert_eq!(a.identity(), b.identity());
    let hasher = rustc_hash::FxBuildHasher;
    assert_eq!(hasher.hash_one(&a), hasher.hash_one(&b));
}

#[test]
fn the_grammar_the_start_rule_and_the_size_cap_each_tell_constraints_apart() {
    let base = constraint(AB, "start", 16, "a");
    assert_ne!(base, constraint("start: \"a\" \"c\"*\n", "start", 16, "a"));
    assert_ne!(base, constraint(AB, "start", 32, "a"));
    let two = "start: \"a\" other\nother: \"b\"\n";
    let grammar = compiled(two, "start");
    let start = grammar.rule_id("start").unwrap();
    let other = grammar.rule_id("other").unwrap();
    let from_start = GrammarConstraint::new(grammar.clone(), start, 16, u32::MAX, None).unwrap();
    let from_other = GrammarConstraint::new(grammar, other, 16, u32::MAX, None).unwrap();
    assert_eq!(from_start.simplest(), None);
    assert_eq!(from_start.simplest_or_init(|| Err(7)), Err(7));
    assert_eq!(from_start.simplest(), None);
    assert_eq!(
        from_start.simplest_or_init(|| Ok::<_, u8>(cps("ab"))),
        Ok(cps("ab").as_slice())
    );
    assert_eq!(
        from_start.simplest_or_init(|| Ok::<_, u8>(cps("zzz"))),
        Ok(cps("ab").as_slice())
    );
    from_other
        .simplest_or_init(|| Ok::<_, u8>(cps("b")))
        .unwrap();
    assert_eq!(from_start.simplest(), Some(cps("ab").as_slice()));
    assert_ne!(from_start, from_other);
    assert!(from_start.accepts(&cps("ab")));
    assert!(from_other.accepts(&cps("b")));
    assert!(!from_other.accepts(&cps("ab")));
}

#[test]
fn an_alphabet_holds_every_terminal_but_a_placeholder_and_tells_constraints_apart() {
    let text = "start: WORD \" \" X\nWORD: /[a-zA-Z]+/\n%declare X\n";
    let mut grammar = compiled(text, "start");
    let x = grammar.terminal_id("X").unwrap();
    grammar.terminals[x] = crate::native::lark::compile::TerminalDef::placeholder("X").unwrap();
    let start = grammar.start[0];
    let lower = IntervalSet::new(vec![
        (u32::from(' '), u32::from(' ')),
        (u32::from('a'), u32::from('z')),
    ])
    .unwrap();
    let narrow =
        GrammarConstraint::new(grammar.clone(), start, 64, u32::MAX, Some(&lower)).unwrap();
    let wide = GrammarConstraint::new(grammar, start, 64, u32::MAX, None).unwrap();
    let marker = placeholder("X");
    assert!(narrow.accepts(&cps(&format!("ab {marker}"))));
    assert!(!narrow.accepts(&cps(&format!("aB {marker}"))));
    assert!(wide.accepts(&cps(&format!("aB {marker}"))));
    assert!(
        marker
            .chars()
            .all(|c| narrow.intervals().contains(u32::from(c)))
    );
    assert!(!narrow.intervals().contains(u32::from('!')));
    assert_ne!(narrow, wide);
}

#[test]
fn debug_names_the_identity_and_the_start_rule() {
    let c = constraint(AB, "start", 16, "a");
    let shown = format!("{c:?}");
    assert_eq!(
        shown,
        format!(
            "GrammarConstraint {{ identity: {:016x}, start: \"start\" }}",
            c.identity()
        )
    );
}

/// A string with a placeholder right beside a token is not one the
/// grammar draws when it ignores a terminal, and is when it does not.
#[test]
fn placeholders_are_kept_apart_from_tokens_where_the_grammar_ignores_text() {
    let spec = |text: &str| {
        from_lark(
            text,
            &NoImports,
            GrammarOptions {
                explicit: vec!["X".to_string()],
                ..GrammarOptions::default()
            },
        )
        .unwrap()
    };
    let marker = placeholder("X");
    let with_ws = spec("start: \"a\" X \"b\"\n%declare X\n%import common.WS\n%ignore WS\n");
    let c = with_ws.constraint();
    assert!(c.accepts(&cps(&format!("a {marker} b"))));
    assert!(c.accepts(&cps(&format!(" a {marker} b "))));
    assert!(!c.accepts(&cps(&format!("a{marker} b"))));
    assert!(!c.accepts(&cps(&format!("a {marker}b"))));
    assert!(!c.accepts(&cps(&format!("a{marker}b"))));
    let tree = c.tree(&cps(&format!("a  {marker} b"))).unwrap();
    assert_eq!(
        c.separators(&Layout::new(&tree)),
        vec![(1, 3), (3 + marker.len(), 4 + marker.len())]
    );
    let plain = spec("start: \"a\" X \"b\"\n%declare X\n");
    assert!(plain.constraint().accepts(&cps(&format!("a{marker}b"))));
    let ignored = spec("start: \"a\" \"b\"\n%declare X\n%ignore X\n");
    assert!(ignored.constraint().accepts(&cps(&format!("a{marker}b"))));
}

#[test]
fn the_memo_is_emptied_when_its_derivations_have_too_many_nodes() {
    let c = constraint("start: /[a-z]+/\n", "start", 16, "a");
    assert_eq!(c.known_len(), 1);
    let nodes = c.tree(&cps("a")).unwrap().size();
    c.cap_known_nodes(3 * nodes);
    assert!(c.accepts(&cps("ab")));
    assert!(c.accepts(&cps("ac")));
    assert_eq!(c.known_len(), 3);
    assert!(c.accepts(&cps("ad")));
    assert_eq!(c.known_len(), 1);
    assert!(c.accepts(&cps("ab")));
    assert_eq!(c.known_len(), 2);
}
