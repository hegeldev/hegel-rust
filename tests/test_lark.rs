mod common;

use common::utils::{assert_all_examples, expect_panic, find_any, minimal};
use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Settings, TestCase};

const INT_LIST: &str = "start: INT (\",\" INT)*\n%import common.INT\n";

fn is_int_list(s: &str) -> bool {
    s.split(',')
        .all(|part| part.parse::<u64>().is_ok() || part.len() > 19)
}

#[test]
fn test_from_lark_generates_strings_of_the_grammar() {
    assert_all_examples(gs::from_lark(INT_LIST), |s: &String| is_int_list(s));
}

#[test]
fn test_from_lark_generates_large_strings_by_default() {
    find_any(gs::from_lark(INT_LIST), |s: &String| s.len() > 2000);
}

#[test]
fn test_from_lark_shrinks_to_the_smallest_string() {
    assert_eq!(
        minimal(gs::from_lark(INT_LIST), |s: &String| s.contains(',')),
        "0,0"
    );
}

#[test]
fn test_from_lark_start_selects_the_rule() {
    assert_all_examples(
        gs::from_lark("a: \"a\"+\nb: \"b\"+\n").start("b"),
        |s: &String| !s.is_empty() && s.chars().all(|c| c == 'b'),
    );
}

#[test]
fn test_from_lark_explicit_supplies_declared_terminals() {
    assert_all_examples(
        gs::from_lark("start: WORD \"!\" NUM\n%declare WORD NUM\n")
            .explicit("WORD", gs::from_regex("[a-z]{3}"))
            .explicit("NUM", gs::text().alphabet("0123456789").min_size(1)),
        |s: &String| {
            let (word, num) = s.split_once('!').unwrap();
            word.len() == 3
                && word.chars().all(|c| c.is_ascii_lowercase())
                && !num.is_empty()
                && num.chars().all(|c| c.is_ascii_digit())
        },
    );
}

#[test]
fn test_from_lark_alphabet_constrains_pattern_terminals() {
    assert_all_examples(
        gs::from_lark("start: /[a-z]+/").alphabet(
            gs::characters()
                .min_codepoint(u32::from('a'))
                .max_codepoint(u32::from('b')),
        ),
        |s: &String| !s.is_empty() && s.chars().all(|c| c == 'a' || c == 'b'),
    );
}

#[test]
fn test_from_lark_max_size_caps_the_length() {
    assert_all_examples(gs::from_lark(INT_LIST).max_size(10), |s: &String| {
        is_int_list(s) && s.len() <= 10
    });
}

#[test]
fn test_from_lark_max_depth_caps_the_nesting() {
    assert_all_examples(
        gs::from_lark("start: \"(\" start \")\" | \"x\"").max_depth(3),
        |s: &String| ["x", "(x)", "((x))"].contains(&s.as_str()),
    );
    find_any(
        gs::from_lark("start: \"(\" start \")\" | \"x\"").max_depth(3),
        |s: &String| s == "((x))",
    );
}

#[test]
fn test_from_lark_import_path_resolves_imports() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("lib.lark"), "LETTERS: /[a-z]+/\n").unwrap();
    assert_all_examples(
        gs::from_lark("%import lib.LETTERS\nstart: LETTERS\n").import_path(dir.path()),
        |s: &String| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase()),
    );
}

#[test]
fn test_from_lark_import_path_replaces_the_previous_one() {
    let earlier = tempfile::TempDir::new().unwrap();
    let later = tempfile::TempDir::new().unwrap();
    std::fs::write(earlier.path().join("lib.lark"), "LETTERS: /[a-z]+/\n").unwrap();
    std::fs::write(later.path().join("lib.lark"), "LETTERS: /[A-Z]+/\n").unwrap();
    assert_all_examples(
        gs::from_lark("%import lib.LETTERS\nstart: LETTERS\n")
            .import_path(earlier.path())
            .import_path(later.path()),
        |s: &String| !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase()),
    );
}

#[test]
fn test_from_lark_inserts_ignored_terminals() {
    find_any(
        gs::from_lark("start: \"a\" \"b\"\n%import common.WS\n%ignore WS\n"),
        |s: &String| s.contains(' '),
    );
}

#[test]
fn test_from_lark_draws_inside_collections() {
    assert_all_examples(
        gs::vecs(gs::from_lark(INT_LIST).max_size(20)).max_size(3),
        |v: &Vec<String>| v.len() <= 3 && v.iter().all(|s| is_int_list(s)),
    );
}

#[test]
fn test_from_lark_labels_distinguish_grammars_and_start_rules() {
    let a = gs::from_lark("start: \"a\"");
    let b = gs::from_lark("start: \"b\"");
    assert_ne!(a.label(), b.label());
    assert_ne!(
        a.label(),
        gs::from_lark("start: \"a\"").start("other").label()
    );
    assert_eq!(a.label(), gs::from_lark("start: \"a\"").label());
}

#[test]
fn test_from_lark_explicit_overrides_a_pattern_terminal() {
    assert_all_examples(
        gs::from_lark("start: \"a\" NUM\nNUM: /[0-9]+/\n")
            .explicit("NUM", gs::just("xyz".to_string())),
        |s: &String| s == "axyz",
    );
}

#[test]
fn test_from_lark_explicit_values_are_spliced_in_as_drawn() {
    let marker = "<<EXPLICIT_NUM_392f5a5e9326>>".to_string();
    assert_all_examples(
        gs::from_lark("start: \"a\" NUM\n%declare NUM\n").explicit("NUM", gs::just(marker.clone())),
        move |s: &String| *s == format!("a{marker}"),
    );
}

#[test]
fn test_from_lark_explicit_takes_any_string_generator() {
    let grammar = || {
        gs::from_lark("start: \"[\" NUM (\",\" NUM)* \"]\"\n%declare NUM\n")
            .explicit("NUM", gs::integers::<u8>().map(|n| n.to_string()))
    };
    assert_all_examples(grammar(), |s: &String| {
        let inner = s.strip_prefix('[').unwrap().strip_suffix(']').unwrap();
        inner.split(',').all(|part| part.parse::<u8>().is_ok())
    });
    find_any(grammar(), |s: &String| {
        let inner = &s[1..s.len() - 1];
        let mut parts = inner.split(',');
        let first = parts.next().unwrap();
        parts.any(|part| part != first)
    });
}

#[test]
fn test_from_lark_explicit_values_shrink_with_the_grammar() {
    assert_eq!(
        minimal(
            gs::from_lark("start: NUM (\",\" NUM)*\n%declare NUM\n")
                .explicit("NUM", gs::integers::<u32>().map(|n| n.to_string())),
            |s: &String| s.contains(',')
        ),
        "0,0"
    );
}

#[test]
fn test_from_lark_explicit_values_get_ignored_text_either_side() {
    assert_all_examples(
        gs::from_lark(
            "start: \"let\" NAME \"=\" NUM\n%declare NAME\nNUM: /[0-9]+/\n%import common.WS\n%ignore WS\n",
        )
        .explicit("NAME", gs::text().alphabet("abc").max_size(3)),
        |s: &String| {
            let rest = s.trim_start().strip_prefix("let").unwrap();
            let (name, after) = rest.split_once('=').unwrap();
            name.starts_with(char::is_whitespace)
                && name.ends_with(char::is_whitespace)
                && name.trim().chars().all(|c| "abc".contains(c))
                && !after.trim().is_empty()
                && after.trim().chars().all(|c| c.is_ascii_digit())
        },
    );
}

/// The values spliced in count toward `max_size` like the grammar's own
/// text, and the cap is on the string with them in, not on the markers.
#[test]
fn test_from_lark_max_size_counts_explicit_values() {
    let grammar = || {
        gs::from_lark("start: NUM (\",\" NUM)*\n%declare NUM\n")
            .explicit(
                "NUM",
                gs::text().alphabet("0123456789").min_size(1).max_size(6),
            )
            .max_size(12)
    };
    assert_all_examples(grammar(), |s: &String| s.len() <= 12);
    find_any(grammar(), |s: &String| s.len() >= 10);
    find_any(grammar(), |s: &String| s.matches(',').count() >= 3);
}

/// A string of nothing but explicit values is cut down to fit by drawing
/// fewer of them.
#[test]
fn test_from_lark_fits_a_string_of_explicit_values_alone() {
    let grammar = || {
        gs::from_lark("start: NUM+\n%declare NUM\n")
            .explicit(
                "NUM",
                gs::text().alphabet("0123456789").min_size(4).max_size(4),
            )
            .max_size(20)
    };
    assert_all_examples(grammar(), |s: &String| s.len() <= 20 && s.len() % 4 == 0);
    find_any(grammar(), |s: &String| s.len() == 20);
}

#[test]
fn test_from_lark_explicit_values_go_beside_text_like_a_marker() {
    assert_all_examples(
        gs::from_lark("start: \"<\" NUM \">\" \"<<\"\n%declare NUM\n").explicit(
            "NUM",
            gs::text().alphabet("0123456789").min_size(1).max_size(3),
        ),
        |s: &String| {
            let inner = s.strip_prefix('<').unwrap().strip_suffix("><<").unwrap();
            !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit())
        },
    );
}

/// A string whose values cannot fit under `max_size` however little the
/// grammar's own text takes is rejected, as a failed filter is.
#[test]
fn test_from_lark_rejects_a_string_whose_explicit_values_cannot_fit() {
    expect_panic(
        || {
            Hegel::new(|tc: TestCase| {
                tc.draw(
                    gs::from_lark("start: NUM\n%declare NUM\n")
                        .explicit("NUM", gs::text().min_size(20))
                        .max_size(10),
                );
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "FilterTooMuch",
    );
}

#[test]
#[should_panic(expected = "explicit terminal 'NUM' is given more than one generator")]
fn test_from_lark_rejects_a_terminal_given_two_generators() {
    find_any(
        gs::from_lark("start: NUM\n%declare NUM\n")
            .explicit("NUM", gs::text())
            .explicit("NUM", gs::text()),
        |_: &String| true,
    );
}

#[test]
fn test_from_lark_labels_include_the_explicit_generators() {
    let grammar = "start: NUM\n%declare NUM\n";
    let a = gs::from_lark(grammar).explicit("NUM", gs::text());
    let b = gs::from_lark(grammar).explicit("NUM", gs::from_regex("[0-9]+"));
    assert_ne!(a.label(), b.label());
    assert_ne!(a.label(), gs::from_lark(grammar).label());
}
