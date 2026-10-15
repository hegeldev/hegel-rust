//! Tests for `src/native/lark/lexsim.rs`: the simulated basic lexer's
//! terminal order and keyword re-typing, the dynamic lexer's per-terminal
//! matching, and the first-character analysis that speeds both up.

use super::*;
use crate::native::lark::builder::{NoImports, load_grammar};
use crate::native::lark::compile::{Grammar, compile};
use crate::native::lark::escape::pattern_to_regexp;
use crate::native::re::constants::SRE_FLAG_UNICODE;
use crate::native::re::matcher::Matcher;
use crate::native::re::parser::parse_pattern;
use alloc::vec;

fn compiled(text: &str) -> Grammar {
    let built = load_grammar(text, &NoImports).unwrap();
    compile(&built, &["start"]).unwrap()
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn lex<'g>(grammar: &'g Grammar, text: &str) -> Vec<(&'g str, usize)> {
    let lexer = Lexer::new(&grammar.terminals);
    let text = chars(text);
    let mut pos = 0;
    let mut out = vec![];
    while pos < text.len() {
        let Lexed::Token(id, end) = lexer.next_token(&text, pos) else {
            panic!("no token at {pos}");
        };
        if !grammar.ignore.contains(&id) {
            out.push((grammar.terminals[id].name.as_str(), end - pos));
        }
        pos = end;
    }
    out
}

fn firsts(pattern: &str) -> Option<FirstChars> {
    first_chars(&parse_pattern(pattern, 0).unwrap())
}

fn ranges(pattern: &str) -> Vec<(u32, u32)> {
    firsts(pattern).unwrap().0
}

#[test]
fn keyword_matched_by_a_regex_is_retyped() {
    let g = compiled("start: NAME \"if\" \"while\"\nNAME: /[a-z]+/\n%ignore \" \"\n");
    assert_eq!(
        lex(&g, "if while iffy"),
        vec![("IF", 2), ("WHILE", 5), ("NAME", 4)]
    );
}

#[test]
fn keyword_at_a_different_priority_is_not_retyped() {
    let g = compiled("start: NAME KW\nNAME: /[a-z]+/\nKW.2: \"if\"\n%ignore \" \"\n");
    assert_eq!(lex(&g, "iffy if"), vec![("KW", 2), ("NAME", 2), ("KW", 2)]);
}

#[test]
fn keyword_with_flags_the_regex_lacks_stays_in_the_order() {
    let g = compiled("start: NAME KW\nNAME: /[a-z]+/\nKW: \"if\"i\n%ignore \" \"\n");
    assert_eq!(lex(&g, "IF if"), vec![("KW", 2), ("KW", 2)]);
}

#[test]
fn string_terminals_with_flags_are_retyped_by_comparing_their_literals() {
    let g = compiled("start: NAME KW\nNAME: /[a-z]+/i\nKW: \"if\"i\n%ignore \" \"\n");
    assert_eq!(
        lex(&g, "IF iffy If"),
        vec![("KW", 2), ("NAME", 4), ("KW", 2)]
    );
    let texts = [
        "KeyWord", "keyword", "KEYWORD", "KeyWor", "KeyWords", "", "a b", "A B", "ab", "ß", "ẞ",
        "k", "K", "\u{212A}", "Ab", "aB",
    ];
    let strings: &[(&str, &str)] = &[
        ("KeyWord", "i"),
        ("a b", "x"),
        ("ß", "i"),
        ("k", "i"),
        ("Ab", "is"),
        ("ab", ""),
    ];
    for &(value, flags) in strings {
        let pattern = Pattern::str(value).with_flags(flags);
        let parsed = parse_pattern(&pattern_to_regexp(&pattern), 0).unwrap();
        let term = TermPattern::new(&pattern, parsed.clone());
        assert_eq!(term.literal.is_some(), flags.is_empty());
        assert_eq!(term.literals.is_some(), !flags.is_empty());
        for text in texts {
            let text = chars(text);
            assert_eq!(
                term.fullmatch(&text),
                Matcher::new(&parsed).fullmatch(&text),
                "{value:?} with flags {flags:?} on {text:?}"
            );
        }
    }
    let plain = Pattern::re("abc");
    let parsed = parse_pattern(&pattern_to_regexp(&plain), 0).unwrap();
    let term = TermPattern::new(&plain, parsed);
    assert!(term.literal.is_none());
    assert_eq!(term.literals, Some((vec![97, 98, 99], SRE_FLAG_UNICODE)));
    let regex = Pattern::re("a.c");
    let parsed = parse_pattern(&pattern_to_regexp(&regex), 0).unwrap();
    let term = TermPattern::new(&regex, parsed);
    assert!(term.literals.is_none());
    assert_eq!(term.fullmatch(&chars("abc")), Ok(true));
    assert_eq!(term.fullmatch(&chars("axc")), Ok(true));
    assert_eq!(term.fullmatch(&chars("abcd")), Ok(false));
    assert_eq!(term.fullmatch(&chars("ab")), Ok(false));
}

#[test]
fn runs_and_probes_the_backtracking_matcher_left_open_are_carried() {
    let grammar = compiled(
        "start: NUM+ | D | OPT | BS | C | DA\nNUM: /[0-9]/\nD: /(?!x)[0-9]+/\nOPT: /(?!x)ab?/\n\
         BS: /b+/\nC: /c/\nDA: /[0-9a]/\n",
    );
    let lexer = Lexer::new(&grammar.terminals);
    let id = |name: &str| grammar.terminal_id(name).unwrap();
    let digits = lexer.match_terminal_scan(id("D"), &chars("12"), 0);
    assert_eq!(digits.lexed, Lexed::Token(id("D"), 2));
    assert!(matches!(digits.steps.as_slice(), [(_, Carry::Tail(_))]));
    assert!(!lexer.can_stop(&digits, id("NUM")));
    assert!(lexer.can_stop(&digits, id("DA")));
    assert!(lexer.can_stop(&digits, id("C")));
    assert_eq!(lexer.settle(&digits, &chars("123"), 0, 2, false), None);
    assert_eq!(
        lexer.settle(&digits, &chars("12x"), 0, 2, false),
        Some(Lexed::Token(id("D"), 2))
    );
    assert_eq!(
        lexer.extend(&digits, &chars("123"), 0, 2, false),
        Some(Lexed::Token(id("D"), 3))
    );
    let opt = lexer.match_terminal_scan(id("OPT"), &chars("a"), 0);
    assert_eq!(opt.lexed, Lexed::Token(id("OPT"), 1));
    assert!(matches!(opt.steps.as_slice(), [(_, Carry::Unless(_))]));
    assert!(!lexer.can_stop(&opt, id("BS")));
    assert!(lexer.can_stop(&opt, id("C")));
    assert_eq!(lexer.settle(&opt, &chars("a"), 0, 1, false), None);
    assert_eq!(lexer.settle(&opt, &chars("ab"), 0, 1, false), None);
    assert_eq!(
        lexer.settle(&opt, &chars("ac"), 0, 1, false),
        Some(Lexed::Token(id("OPT"), 1))
    );
    assert_eq!(lexer.extend(&opt, &chars("ab"), 0, 1, false), None);
    assert_eq!(
        lexer.extend(&opt, &chars("ac"), 0, 1, false),
        Some(Lexed::Token(id("OPT"), 1))
    );
}

#[test]
fn longer_possible_match_comes_first() {
    let g = compiled("start: A B\nA: /x/\nB: /x[0-9]?/\n");
    assert_eq!(lex(&g, "xx1"), vec![("B", 1), ("B", 2)]);
}

#[test]
fn longer_pattern_source_breaks_width_ties_then_name() {
    let g = compiled("start: A B C\nA: /a|b/\nB: /[ab]/\nC: /b/\n");
    assert_eq!(lex(&g, "ab"), vec![("B", 1), ("B", 1)]);
    let g = compiled("start: B A\nA: /[ab]/\nB: /[ba]/\n");
    assert_eq!(lex(&g, "a"), vec![("A", 1)]);
}

#[test]
fn priority_beats_width() {
    let g = compiled("start: A B\nA.1: /a/\nB: /a+/\n");
    assert_eq!(lex(&g, "aa"), vec![("A", 1), ("A", 1)]);
}

#[test]
fn the_lowest_priority_there_is_comes_last() {
    let g = compiled("start: A B\nA.-9223372036854775808: /a/\nB: /a+/\n");
    assert_eq!(lex(&g, "aa"), vec![("B", 2)]);
}

#[test]
fn unlexable_text_is_no_match() {
    let g = compiled("start: A\nA: /a/\n");
    let lexer = Lexer::new(&g.terminals);
    assert_eq!(lexer.next_token(&chars("b"), 0), Lexed::NoMatch);
    assert_eq!(lexer.next_token(&chars("a"), 1), Lexed::NoMatch);
}

#[test]
fn declared_terminals_take_no_part_in_the_order() {
    let g = compiled("start: A B\nA: /a/\n%declare B\n");
    let lexer = Lexer::new(&g.terminals);
    let b = g.terminal_id("B").unwrap();
    assert_eq!(lexer.match_terminal(b, &chars("a"), 0), Lexed::Unknown);
    assert_eq!(lex(&g, "a"), vec![("A", 1)]);
}

#[test]
fn match_terminal_reports_its_own_pattern_only() {
    let g = compiled("start: A B\nA: /a+/\nB: /ab/\n");
    let lexer = Lexer::new(&g.terminals);
    let a = g.terminal_id("A").unwrap();
    let b = g.terminal_id("B").unwrap();
    assert_eq!(
        lexer.match_terminal(a, &chars("aab"), 0),
        Lexed::Token(a, 2)
    );
    assert_eq!(
        lexer.match_terminal(b, &chars("aab"), 1),
        Lexed::Token(b, 3)
    );
    assert_eq!(lexer.match_terminal(b, &chars("aab"), 0), Lexed::NoMatch);
}

#[test]
fn matcher_running_out_of_budget_is_unknown() {
    let g = compiled("start: A\nA: /(a*)*b\\1/\n");
    let lexer = Lexer::new(&g.terminals);
    let text = chars(&"a".repeat(40));
    assert_eq!(lexer.match_terminal(0, &text, 0), Lexed::Unknown);
    assert_eq!(lexer.next_token(&text, 0), Lexed::Unknown);
}

#[test]
fn first_chars_of_literals_sets_and_branches() {
    assert_eq!(ranges("abc"), vec![(97, 97)]);
    assert_eq!(ranges("[a-cx]|b|z"), vec![(97, 99), (120, 120), (122, 122)]);
    assert_eq!(ranges("(?:q|r)s"), vec![(113, 114)]);
    assert_eq!(ranges("(?>t)u"), vec![(116, 116)]);
    assert_eq!(ranges("(?!x)y"), vec![(121, 121)]);
    assert_eq!(ranges("^\\Ab"), vec![(98, 98)]);
}

#[test]
fn first_chars_skip_nullable_prefixes() {
    assert_eq!(ranges("a*b"), vec![(97, 98)]);
    assert_eq!(ranges("a?b"), vec![(97, 98)]);
    assert_eq!(ranges("(a|)b"), vec![(97, 98)]);
    assert_eq!(ranges("a{2,}"), vec![(97, 97)]);
    assert_eq!(ranges("a*?b"), vec![(97, 98)]);
    assert_eq!(ranges("a*+b"), vec![(97, 98)]);
}

#[test]
fn first_chars_give_up_on_unknowable_starts() {
    assert_eq!(firsts("[^a]b"), None);
    assert_eq!(firsts("[^ab]c"), None);
    assert_eq!(firsts(".b"), None);
    assert_eq!(firsts("\\D"), None);
    assert_eq!(firsts("\\S"), None);
    assert_eq!(firsts("\\W"), None);
    assert_eq!(firsts("(a)\\1"), Some(FirstChars(vec![(97, 97)])));
    assert_eq!(firsts("(a)?\\1b"), None);
    assert_eq!(firsts("(?i:a)b"), None);
    assert_eq!(firsts("(?i)ab"), None);
    assert_eq!(firsts("a*"), None);
    assert_eq!(firsts("(?!)"), Some(FirstChars(vec![])));
}

#[test]
fn first_chars_of_category_classes_cover_the_class() {
    let digits = firsts("\\d").unwrap();
    assert!(digits.contains('7') && digits.contains('\u{663}') && !digits.contains('a'));
    let spaces = firsts("\\s*//").unwrap();
    assert!(spaces.contains(' ') && spaces.contains('\u{3000}') && spaces.contains('/'));
    assert!(spaces.contains('\u{1c}') && !spaces.contains('x'));
    let words = firsts("\\w+").unwrap();
    assert!(words.contains('_') && words.contains('\u{e9}') && words.contains('\u{663}'));
    assert!(!words.contains('-') && !words.contains(' '));
}

#[test]
fn first_chars_of_conditional_groups() {
    assert_eq!(ranges("(a)?(?(1)b|c)"), vec![(97, 99)]);
    assert_eq!(firsts("(a)?(?(1)b)"), None);
    assert_eq!(ranges("(a)(?(1)b|c)"), vec![(97, 97)]);
}

#[test]
fn first_chars_merge_adjacent_and_overlapping_ranges() {
    assert_eq!(ranges("[a-c]|[d-f]|[e-h]|z"), vec![(97, 104), (122, 122)]);
    let first = firsts("[a-c]|z").unwrap();
    assert!(first.contains('b'));
    assert!(first.contains('z'));
    assert!(!first.contains('d'));
    assert!(!first.contains('{'));
}

#[test]
fn first_chars_prune_terminals_in_the_order() {
    let g = compiled("start: A B\nA: /a+/\nB: /[^a]+/\n");
    assert_eq!(lex(&g, "aab"), vec![("A", 2), ("B", 1)]);
}

#[test]
fn extend_carries_what_the_end_of_the_text_left_open() {
    let grammar = compiled(
        "start: NAME KW C D E F\nNAME: /[a-z]+/\nKW: \"if\"\nC: \"ab\"\nD: /(?!x)[0-9]+/\nE: /a\\b/\nF: /(?<=x)(a?b)+/\n",
    );
    let lexer = Lexer::new(&grammar.terminals);
    let id = |name: &str| {
        grammar
            .terminals
            .iter()
            .position(|t| t.name == name)
            .unwrap()
    };
    let scan = lexer.next_token_scan(&chars("i"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("NAME"), 1));
    assert_eq!(
        lexer.extend(&scan, &chars("if "), 0, 1, true),
        Some(Lexed::Token(id("KW"), 2))
    );
    assert_eq!(
        lexer.extend(&scan, &chars("ifx"), 0, 1, true),
        Some(Lexed::Token(id("NAME"), 3))
    );
    assert_eq!(
        lexer.extend(&scan, &chars("xif"), 1, 1, false),
        Some(Lexed::Token(id("NAME"), 3))
    );
    let scan = lexer.match_terminal_scan(id("C"), &chars("a"), 0);
    assert_eq!(scan.lexed, Lexed::NoMatch);
    assert_eq!(
        lexer.extend(&scan, &chars("ab"), 0, 1, false),
        Some(Lexed::Token(id("C"), 2))
    );
    assert_eq!(
        lexer.extend(&scan, &chars("ax"), 0, 1, false),
        Some(Lexed::NoMatch)
    );
    let scan = lexer.match_terminal_scan(id("C"), &chars("x"), 0);
    assert_eq!(
        lexer.extend(&scan, &chars("xab"), 0, 1, false),
        Some(Lexed::NoMatch)
    );
    let scan = lexer.match_terminal_scan(id("D"), &chars("12"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("D"), 2));
    assert_eq!(
        lexer.extend(&scan, &chars("123-"), 0, 2, false),
        Some(Lexed::Token(id("D"), 3))
    );
    let scan = lexer.match_terminal_scan(id("E"), &chars("a"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("E"), 1));
    assert_eq!(lexer.extend(&scan, &chars("ab"), 0, 1, false), None);
    let scan = lexer.match_terminal_scan(id("F"), &chars("b"), 0);
    assert_eq!(scan.lexed, Lexed::NoMatch);
    assert_eq!(lexer.extend(&scan, &chars("bb"), 0, 1, false), None);
    assert!(
        lexer
            .extend(
                &lexer.match_terminal_scan(id("F"), &chars("xb"), 1),
                &chars("xbb"),
                1,
                1,
                false
            )
            .is_some()
    );
    let scan = lexer.match_terminal_scan(id("C"), &chars("ab"), 2);
    assert_eq!(scan.lexed, Lexed::NoMatch);
    assert_eq!(lexer.extend(&scan, &chars("abab"), 2, 0, false), None);
    assert_eq!(
        lexer.extend(
            &lexer.next_token_scan(&chars("a"), 1),
            &chars("ab"),
            1,
            0,
            false
        ),
        None
    );
}

#[test]
fn anchored_patterns_may_look_before_their_start() {
    let anchored_pattern = |pattern: &str| anchored(&parse_pattern(pattern, 0).unwrap());
    for pattern in [
        "^a",
        "\\Aa",
        "\\ba",
        "\\Ba",
        "(?<=x)a",
        "a(?<=ab)c",
        "a?(?<=a)b",
        "(a|bc)(?<=bc)x",
        "(?=\\ba)a",
        "(\\ba)+",
        "a*\\b",
        "(a|)\\b",
        "(a)?(?(1)b)\\b",
        "(?<=aa)b",
        "a(?<=aa)b",
        "(a)(?<=\\1)b",
        "a*+\\b",
    ] {
        assert!(anchored_pattern(pattern), "{pattern}");
    }
    for pattern in [
        "a\\b",
        "a$",
        "a\\Z",
        "$",
        "a(?<=a)b",
        "(a|bc)(?<=c)x",
        "a(?=\\b)",
        "(?!x)a",
        "(a\\b)+",
        "a{2}\\b",
        "(a)\\1\\b",
        "(a)?(?(1)b|cd)\\b",
        "(?>a)\\b",
        "[ab]\\b",
        ".\\b",
        "[^a]\\b",
        "(?!)a",
    ] {
        assert!(!anchored_pattern(pattern), "{pattern}");
    }
}

#[test]
fn scans_note_whether_an_anchored_terminal_was_tried() {
    let grammar = compiled("start: A B C D\nA: /\\b[ab]a/\nB.2: /b/\nC: /(?<=x)c|d/\nD: /\\bbb/\n");
    let lexer = Lexer::new(&grammar.terminals);
    let id = |name: &str| {
        grammar
            .terminals
            .iter()
            .position(|t| t.name == name)
            .unwrap()
    };
    let scan = lexer.next_token_scan(&chars("aa"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("A"), 2));
    assert!(scan.anchored);
    let scan = lexer.next_token_scan(&chars("d"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("C"), 1));
    assert!(scan.anchored);
    let scan = lexer.next_token_scan(&chars("b"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("B"), 1));
    assert!(!scan.anchored);
    let scan = lexer.next_token_scan(&chars("bc"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("B"), 1));
    assert!(!scan.anchored);
    let scan = lexer.next_token_scan(&chars("ba"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("B"), 1));
    assert!(!scan.anchored);
    assert!(lexer.match_terminal_scan(id("A"), &chars("ax"), 0).anchored);
    assert!(!lexer.match_terminal_scan(id("A"), &chars("x"), 0).anchored);
    assert!(lexer.match_terminal_scan(id("D"), &chars("bb"), 0).anchored);
    assert!(!lexer.match_terminal_scan(id("B"), &chars("b"), 0).anchored);
}

#[test]
fn settle_answers_only_what_more_text_cannot_change() {
    let grammar = compiled("start: NAME C F\nNAME: /[a-z]+/\nC: \"ab\"\nF: /(a?b)+/\n");
    let lexer = Lexer::new(&grammar.terminals);
    let id = |name: &str| {
        grammar
            .terminals
            .iter()
            .position(|t| t.name == name)
            .unwrap()
    };
    let scan = lexer.match_terminal_scan(id("NAME"), &chars("ab"), 0);
    assert_eq!(
        lexer.settle(&scan, &chars("ab1"), 0, 2, false),
        Some(Lexed::Token(id("NAME"), 2))
    );
    assert_eq!(lexer.settle(&scan, &chars("abc"), 0, 2, false), None);
    assert_eq!(
        lexer.extend(&scan, &chars("abc"), 0, 2, false),
        Some(Lexed::Token(id("NAME"), 3))
    );
    assert_eq!(lexer.settle(&scan, &chars("ab"), 0, 2, false), None);
    let scan = lexer.match_terminal_scan(id("C"), &chars("a"), 0);
    assert_eq!(scan.lexed, Lexed::NoMatch);
    assert_eq!(lexer.settle(&scan, &chars("a"), 0, 1, false), None);
    assert_eq!(
        lexer.settle(&scan, &chars("ax"), 0, 1, false),
        Some(Lexed::NoMatch)
    );
    assert_eq!(
        lexer.settle(&scan, &chars("ab"), 0, 1, false),
        Some(Lexed::Token(id("C"), 2))
    );
    let scan = lexer.match_terminal_scan(id("F"), &chars("ab"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("F"), 2));
    assert_eq!(
        lexer.settle(&scan, &chars("abx"), 0, 2, false),
        Some(Lexed::Token(id("F"), 2))
    );
    assert_eq!(lexer.settle(&scan, &chars("aba"), 0, 2, false), None);
    assert_eq!(lexer.settle(&scan, &chars("abab"), 0, 2, false), None);
    assert_eq!(
        lexer.extend(&scan, &chars("abab"), 0, 2, false),
        Some(Lexed::Token(id("F"), 4))
    );
}

#[test]
fn tests_that_found_the_end_are_run_again_on_the_next_character() {
    let grammar = compiled(
        "start: NUMBER COMMENT TAIL
NUMBER: /[0-9]+(\\.[0-9])?/
COMMENT: /\\s*#[^\\n]*/
TAIL: /[a-z]+(?<=a)/
",
    );
    let lexer = Lexer::new(&grammar.terminals);
    let id = |name: &str| {
        grammar
            .terminals
            .iter()
            .position(|t| t.name == name)
            .unwrap()
    };
    let scan = lexer.match_terminal_scan(id("NUMBER"), &chars("12"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("NUMBER"), 2));
    for text in ["12,", "12 x"] {
        assert_eq!(
            lexer.settle(&scan, &chars(text), 0, 2, false),
            Some(Lexed::Token(id("NUMBER"), 2)),
            "{text:?}"
        );
    }
    for text in ["123", "12"] {
        assert_eq!(
            lexer.settle(&scan, &chars(text), 0, 2, false),
            None,
            "{text:?}"
        );
    }
    assert_eq!(
        lexer.settle(&scan, &chars("12.5"), 0, 2, false),
        Some(Lexed::Token(id("NUMBER"), 4))
    );
    assert_eq!(
        lexer.extend(&scan, &chars("12"), 0, 2, false),
        Some(Lexed::Token(id("NUMBER"), 2))
    );
    assert_eq!(
        lexer.extend(&scan, &chars("12.5"), 0, 2, false),
        Some(Lexed::Token(id("NUMBER"), 4))
    );
    let scan = lexer.match_terminal_scan(id("COMMENT"), &chars("  "), 0);
    assert_eq!(scan.lexed, Lexed::NoMatch);
    assert_eq!(
        lexer.settle(&scan, &chars("  x"), 0, 2, false),
        Some(Lexed::NoMatch)
    );
    assert_eq!(lexer.settle(&scan, &chars("  #"), 0, 2, false), None);
    assert_eq!(lexer.settle(&scan, &chars("   "), 0, 2, false), None);
    let scan = lexer.next_token_scan(&chars("  "), 0);
    assert_eq!(scan.lexed, Lexed::NoMatch);
    assert_eq!(
        lexer.extend(&scan, &chars("  z"), 0, 2, true),
        Some(Lexed::NoMatch)
    );
    assert_eq!(
        lexer.extend(&scan, &chars("  # c"), 0, 2, true),
        Some(Lexed::Token(id("COMMENT"), 5))
    );
    let scan = lexer.match_terminal_scan(id("TAIL"), &chars("abc"), 0);
    assert_eq!(scan.lexed, Lexed::Token(id("TAIL"), 1));
    assert_eq!(
        lexer.settle(&scan, &chars("abc-"), 0, 3, false),
        Some(Lexed::Token(id("TAIL"), 1))
    );
    assert_eq!(lexer.settle(&scan, &chars("abcd"), 0, 3, false), None);
    assert_eq!(lexer.extend(&scan, &chars("abcd"), 0, 3, false), None);
}

#[test]
fn only_candidates_that_can_end_the_previous_match_are_worth_trying() {
    let grammar = compiled(
        "start: NUM+ | D | COMMENT | WS | NL | OPT | BS | C | WIDE | X\nNUM: /[0-9]/\n\
         COMMENT: /#[^\\n]*/\nWS: /[ \\t]+/\nNL: /\\n+/\nOPT: /ab?/\nBS: /b+/\nC: /c/\n\
         WIDE: /[\\u0100-\\u0fff]/\nX: \"xy\"\n%declare D\n",
    );
    let lexer = Lexer::new(&grammar.terminals);
    let id = |name: &str| grammar.terminal_id(name).unwrap();
    let comment = lexer.match_terminal_scan(id("COMMENT"), &chars("# hi"), 0);
    assert!(!lexer.can_stop(&comment, id("WS")));
    assert!(lexer.can_stop(&comment, id("NL")));
    assert!(!lexer.can_stop(&comment, id("COMMENT")));
    assert!(lexer.can_stop(&comment, id("WIDE")));
    assert!(lexer.can_stop(&comment, id("D")));
    let opt = lexer.match_terminal_scan(id("OPT"), &chars("a"), 0);
    assert!(!lexer.can_stop(&opt, id("BS")));
    assert!(lexer.can_stop(&opt, id("C")));
    let fixed = lexer.match_terminal_scan(id("X"), &chars("xy"), 0);
    assert!(lexer.can_stop(&fixed, id("WS")));
    let partial = lexer.match_terminal_scan(id("X"), &chars("x"), 0);
    assert!(partial.carried && !partial.steps.as_slice().is_empty());
    assert!(lexer.can_stop(&partial, id("WS")));
    let unknown = lexer.match_terminal_scan(id("D"), &chars("x"), 0);
    assert!(lexer.can_stop(&unknown, id("WS")));
}

#[test]
fn a_known_own_scan_stands_in_for_running_its_pattern_again() {
    let g = compiled("start: A B C\nA: /a+/\nB: /ab/\nC: /(c*)*d\\1/\n");
    let lexer = Lexer::new(&g.terminals);
    let (a, b, c) = (
        g.terminal_id("A").unwrap(),
        g.terminal_id("B").unwrap(),
        g.terminal_id("C").unwrap(),
    );
    for (id, text) in [(a, "aab"), (b, "aab"), (b, "abb"), (a, "xa"), (c, "ccccc")] {
        for pos in 0..2 {
            let text = chars(text);
            let own = lexer.match_terminal_scan(id, &text, pos);
            let knowing = lexer.next_token_scan_knowing(&text, pos, id, &own);
            let plain = lexer.next_token_scan(&text, pos);
            assert_eq!(knowing.lexed, plain.lexed, "{id} {text:?} {pos}");
            assert_eq!(knowing.carried, plain.carried, "{id} {text:?} {pos}");
            assert_eq!(knowing.anchored, plain.anchored, "{id} {text:?} {pos}");
            assert_eq!(
                knowing.steps.as_slice().len(),
                plain.steps.as_slice().len(),
                "{id} {text:?} {pos}"
            );
        }
    }
    let long = chars(&"c".repeat(40));
    let own = lexer.match_terminal_scan(c, &long, 0);
    assert_eq!(own.lexed, Lexed::Unknown);
    let knowing = lexer.next_token_scan_knowing(&long, 0, c, &own);
    assert_eq!(knowing.lexed, Lexed::Unknown);
    assert!(!knowing.carried);
}

#[test]
fn steps_stay_in_the_scan_until_a_second_is_noted() {
    let step = |id: TermId| (id, Carry::Tail(id));
    let mut steps = Steps::None;
    assert!(steps.as_slice().is_empty());
    steps.push(1, Carry::Tail(1));
    assert_eq!(steps, Steps::One(step(1)));
    assert_eq!(steps.as_slice(), &[step(1)]);
    steps.push(2, Carry::Tail(2));
    assert_eq!(steps, Steps::Many(vec![step(1), step(2)]));
    steps.push(3, Carry::Tail(3));
    assert_eq!(steps.as_slice(), &[step(1), step(2), step(3)]);
    for (from, expected) in [
        (Steps::None, vec![]),
        (Steps::One(step(4)), vec![step(4)]),
        (Steps::Many(vec![step(5), step(6)]), vec![step(5), step(6)]),
    ] {
        let mut extended = Steps::None;
        extended.extend_from(&from);
        assert_eq!(extended.as_slice(), expected.as_slice());
        let mut onto_one = Steps::One(step(0));
        onto_one.extend_from(&from);
        let mut all = vec![step(0)];
        all.extend(expected);
        assert_eq!(onto_one.as_slice(), all.as_slice());
    }
}

#[test]
fn a_scan_follows_another_that_left_the_same_steps_open() {
    let scan = |lexed: Lexed, steps: Steps, carried: bool| Scan {
        lexed,
        steps,
        carried,
        anchored: false,
    };
    let step = |id: TermId| (id, Carry::Tail(id));
    let own = scan(Lexed::Token(1, 2), Steps::One(step(1)), true);
    assert!(own.follows(&own));
    assert!(scan(Lexed::Token(7, 2), Steps::One(step(1)), true).follows(&own));
    assert!(!scan(Lexed::Token(1, 2), Steps::One(step(1)), false).follows(&own));
    assert!(!scan(Lexed::Token(1, 2), Steps::None, true).follows(&own));
    assert!(
        !scan(
            Lexed::Token(1, 2),
            Steps::Many(vec![step(0), step(1)]),
            true
        )
        .follows(&own)
    );
    assert!(!scan(Lexed::Token(1, 2), Steps::One((1, Carry::Literal)), true).follows(&own));
    let none = scan(Lexed::NoMatch, Steps::None, false);
    assert!(scan(Lexed::Token(1, 2), Steps::None, false).follows(&none));
    assert!(!scan(Lexed::NoMatch, Steps::None, true).follows(&none));

    let grammar = compiled("start: A B C\nA: /ab+/\nB: /a/\nC: /c/\n");
    let lexer = Lexer::new(&grammar.terminals);
    let id = |name: &str| {
        grammar
            .terminals
            .iter()
            .position(|t| t.name == name)
            .unwrap()
    };
    let own = lexer.match_terminal_scan(id("A"), &chars("abb"), 0);
    let basic = lexer.next_token_scan_knowing(&chars("abb"), 0, id("A"), &own);
    assert!(basic.follows(&own));
    let own = lexer.match_terminal_scan(id("B"), &chars("a"), 0);
    let basic = lexer.next_token_scan_knowing(&chars("a"), 0, id("B"), &own);
    assert!(!basic.follows(&own));
    let own = lexer.match_terminal_scan(id("C"), &chars("c"), 0);
    let basic = lexer.next_token_scan_knowing(&chars("c"), 0, id("C"), &own);
    assert!(basic.follows(&own));
}

#[test]
fn starters_rule_terminals_out_by_their_first_character() {
    let g = compiled("start: A B C D\nA: /a+/\nB: /b|c/\nC: /x/i\n%declare D\n");
    let lexer = Lexer::new(&g.terminals);
    let id = |name: &str| g.terminal_id(name).unwrap();
    let (a, b, c, d) = (id("A"), id("B"), id("C"), id("D"));
    let starters = Starters::new(&lexer, &[a, b, c, d]);
    assert_eq!(starters.at('a'), 0b1101);
    assert_eq!(starters.at('b'), 0b1110);
    assert_eq!(starters.at('c'), 0b1110);
    assert_eq!(starters.at('x'), 0b1100);
    assert_eq!(starters.at('\u{e9}'), u64::MAX);
    assert!(Starters::has(starters.at('a'), 0));
    assert!(!Starters::has(starters.at('a'), 1));
    assert!(Starters::has(starters.at('a'), 2));
    assert!(Starters::has(starters.at('a'), 3));
    for ch in 0u8..128 {
        let ch = char::from(ch);
        for (n, &term) in [a, b, c, d].iter().enumerate() {
            if !Starters::has(starters.at(ch), n) {
                assert_eq!(
                    lexer.match_terminal(term, &[ch], 0),
                    Lexed::NoMatch,
                    "{ch:?} {n}"
                );
            }
        }
    }

    let mut many = vec![b; 63];
    many.push(a);
    let starters = Starters::new(&lexer, &many);
    assert_eq!(starters.at('a'), 1 << 63);
    assert_eq!(starters.at('b'), (1 << 63) - 1);
    assert!(Starters::has(starters.at('a'), 63));
    assert!(Starters::has(starters.at('a'), 64));
    assert!(!Starters::has(starters.at('b'), 63));
    assert!(!Starters::has(starters.at('b'), 100));
    let starters = Starters::new(&lexer, &vec![b; 70]);
    assert_eq!(starters.at('a'), 0);
    assert_eq!(starters.at('b'), u64::MAX);
}
