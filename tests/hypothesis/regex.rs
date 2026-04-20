//! Ported from hypothesis-python/tests/cover/test_regex.py

use crate::common::utils::{
    FindAny, assert_all_examples, assert_no_examples, check_can_generate_examples, find_any,
};
use hegel::generators::{self as gs};
use hegel::{HealthCheck, Hegel, Settings};
use regex::Regex;

// test_matching: omitted — validates Python-internal category constants
// (SPACE_CHARS, UNICODE_DIGIT_CATEGORIES, etc.) with no hegel-rust counterpart.

// test_can_generate: parametrized over patterns × {no alphabet, restricted alphabet, bytes}.
// Bytes encode=True skipped (no bytes support). Both string modes are smoke-tested below;
// Rust's regex crate has different Unicode semantics for \d/\w/\s so full verification would
// require a Python-compatible regex engine.
#[test]
fn test_can_generate_patterns_no_alphabet() {
    for pattern in [
        ".",
        "a",
        "abc",
        "[a][b][c]",
        "[^a][^b][^c]",
        "[a-z0-9_]",
        "[^a-z0-9_]",
        "ab?",
        "ab*",
        "ab+",
        "ab{5}",
        "ab{5,10}",
        "ab{,10}",
        "ab{5,}",
        "ab|cd|ef",
        "(foo)+",
        r#"(['\"])[a-z]+\1"#,
        r#"(?:[a-z])(['\"])[a-z]+\1"#,
        r#"(?P<foo>['\"])[a-z]+(?P=foo)"#,
        "^abc",
        r"\d",
        r"[\d]",
        r"[^\D]",
        r"\w",
        r"[\w]",
        r"[^\W]",
        r"\s",
        r"[\s]",
        r"[^\S]",
    ] {
        check_can_generate_examples(gs::from_regex(pattern));
    }
}

#[test]
fn test_can_generate_patterns_with_alphabet() {
    for pattern in [
        ".",
        "a",
        "abc",
        "[a][b][c]",
        "[^a][^b][^c]",
        "[a-z0-9_]",
        "[^a-z0-9_]",
        "ab?",
        "ab*",
        "ab+",
        "ab{5,10}",
        "ab|cd|ef",
        "(foo)+",
        r"\d",
        r"\w",
        r"\s",
    ] {
        check_can_generate_examples(
            gs::from_regex(pattern).alphabet(gs::characters().max_codepoint(1000)),
        );
    }
}

// test_literals_with_ignorecase: patterns with re.IGNORECASE or inline (?i).
// re.compile("\\Aa\\Z", re.IGNORECASE) == "(?i)\\Aa\\Z"
#[test]
fn test_literals_with_ignorecase_a() {
    find_any(gs::from_regex(r"(?i)\Aa\Z"), |s: &String| s == "a");
    find_any(gs::from_regex(r"(?i)\Aa\Z"), |s: &String| s == "A");
}

#[test]
fn test_literals_with_ignorecase_ab() {
    find_any(gs::from_regex(r"(?i)\A[ab]\Z"), |s: &String| s == "a");
    find_any(gs::from_regex(r"(?i)\A[ab]\Z"), |s: &String| s == "A");
}

#[test]
fn test_not_literal_with_ignorecase() {
    assert_all_examples(gs::from_regex(r"(?i)\A[^a][^b]\Z"), |s: &String| {
        let mut chars = s.chars();
        let c0 = chars.next().unwrap();
        let c1 = chars.next().unwrap();
        c0 != 'a' && c0 != 'A' && c1 != 'b' && c1 != 'B'
    });
}

#[test]
fn test_any_doesnt_generate_newline() {
    assert_all_examples(gs::from_regex(r"\A.\Z"), |s: &String| s != "\n");
}

// test_any_with_dotall_generate_newline: re.compile("\\A.\\Z", re.DOTALL) == "(?s)\\A.\\Z"
#[test]
fn test_any_with_dotall_generate_newline() {
    // Under DOTALL `.` draws from the whole BMP-minus-surrogates alphabet.
    // `emit_from_chars` biases 80% of draws into the first 256 codepoints, so
    // `\n` (codepoint 10) lands with ~0.00269 probability per attempt — the
    // default 1000-attempt ceiling only hits ~93% reliability. 10_000 attempts
    // pushes this above 99.999%.
    FindAny::new(gs::from_regex(r"(?s)\A.\Z"), |s: &String| s == "\n")
        .max_attempts(10_000)
        .run();
}

// test_any_with_dotall_generate_newline_binary: omitted — bytes patterns not supported.

// test_groups: omitted — uses Python-internal category predicates (is_word, is_digit, etc.)
// and compiled regex objects; complex parametric test with no direct Rust equivalent.

#[test]
fn test_caret_in_the_middle_does_not_generate_anything() {
    assert_no_examples(gs::from_regex("a^b"), |_: &String| true);
}

#[test]
fn test_end_with_terminator_does_not_pad() {
    assert_all_examples(gs::from_regex(r"abc\Z"), |s: &String| s.ends_with("abc"));
}

#[test]
fn test_end() {
    find_any(gs::from_regex(r"\Aabc$"), |s: &String| s == "abc");
    find_any(gs::from_regex(r"\Aabc$"), |s: &String| s == "abc\n");
}

#[test]
fn test_groupref_exists() {
    assert_all_examples(gs::from_regex("^(<)?a(?(1)>)$"), |s: &String| {
        ["a", "a\n", "<a>", "<a>\n"].contains(&s.as_str())
    });
    assert_all_examples(gs::from_regex("^(a)?(?(1)b|c)$"), |s: &String| {
        ["ab", "ab\n", "c", "c\n"].contains(&s.as_str())
    });
}

#[test]
fn test_impossible_negative_lookahead() {
    assert_no_examples(gs::from_regex("(?!foo)foo"), |_: &String| true);
}

#[test]
fn test_can_handle_boundaries_nested() {
    Hegel::new(|tc| {
        let s: String = tc.draw(gs::from_regex(r"(\Afoo\Z)"));
        assert_eq!(s, "foo");
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_groupref_not_shared_between_regex() {
    Hegel::new(|tc| {
        let _a: String = tc.draw(gs::from_regex(r"(a)\1"));
        let _b: String = tc.draw(gs::from_regex(r"(b)\1"));
    })
    .settings(Settings::new().database(None))
    .run();
}

// test_group_ref_is_not_shared_between_identical_regex: uses base_regex_strategy (internal API).
// test_does_not_leak_groups: uses base_regex_strategy (internal API).

#[test]
fn test_positive_lookbehind() {
    // TooSlow suppressed: .*(?<=ab)c is slow to generate under instrumented binaries.
    FindAny::new(gs::from_regex(".*(?<=ab)c"), |s: &String| {
        s.ends_with("abc")
    })
    .suppress_health_check(HealthCheck::TooSlow)
    .run();
}

#[test]
fn test_positive_lookahead() {
    // TooSlow suppressed: a(?=bc).* is slow to generate under instrumented binaries.
    FindAny::new(gs::from_regex("a(?=bc).*"), |s: &String| {
        s.starts_with("abc")
    })
    .suppress_health_check(HealthCheck::TooSlow)
    .run();
}

#[test]
fn test_negative_lookbehind() {
    assert_all_examples(gs::from_regex("[abc]*(?<!abc)d"), |s: &String| {
        !s.ends_with("abcd")
    });
    assert_no_examples(gs::from_regex("[abc]*(?<!abc)d"), |s: &String| {
        s.ends_with("abcd")
    });
}

#[test]
fn test_negative_lookahead() {
    assert_all_examples(gs::from_regex("^ab(?!cd)[abcd]*"), |s: &String| {
        !s.starts_with("abcd")
    });
    assert_no_examples(gs::from_regex("^ab(?!cd)[abcd]*"), |s: &String| {
        s.starts_with("abcd")
    });
}

#[test]
fn test_generates_only_the_provided_characters_given_boundaries() {
    Hegel::new(|tc| {
        let xs: String = tc.draw(gs::from_regex(r"^a+\Z"));
        assert!(xs.chars().all(|c| c == 'a'));
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_group_backref_may_not_be_present() {
    Hegel::new(|tc| {
        let s: String = tc.draw(gs::from_regex(r"^(.)?\1\Z"));
        assert_eq!(s.chars().count(), 2);
        assert_eq!(s.chars().next(), s.chars().last());
    })
    .settings(Settings::new().database(None))
    .run();
}

#[test]
fn test_subpattern_flags() {
    find_any(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
        s.starts_with('a')
    });
    find_any(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
        s.starts_with('A')
    });
    find_any(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
        s.chars().nth(1) == Some('b')
    });
    assert_no_examples(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
        s.chars().nth(1) == Some('B')
    });
}

// test_can_handle_binary_regex_which_is_not_ascii: omitted — bytes patterns not supported.
// test_regex_have_same_type_as_pattern: bytes variant not supported; string variant is
// trivially true in Rust (from_regex always returns String).

#[test]
fn test_can_pad_strings_arbitrarily() {
    find_any(gs::from_regex("a"), |s: &String| !s.starts_with('a'));
    find_any(gs::from_regex("a"), |s: &String| !s.ends_with('a'));
}

#[test]
fn test_can_pad_empty_strings() {
    find_any(gs::from_regex(""), |s: &String| !s.is_empty());
}

#[test]
fn test_can_pad_strings_with_newlines() {
    find_any(gs::from_regex("^$"), |s: &String| !s.is_empty());
}

// test_given_multiline_regex_can_insert_after_dollar:
// re.compile("\\Ahi$", re.MULTILINE) == "(?m)\\Ahi$"
#[test]
fn test_given_multiline_regex_can_insert_after_dollar() {
    find_any(gs::from_regex(r"(?m)\Ahi$"), |s: &String| {
        s.contains('\n') && s.split('\n').nth(1).is_some_and(|p| !p.is_empty())
    });
}

// test_given_multiline_regex_can_insert_before_caret:
// re.compile("^hi\\Z", re.MULTILINE) == "(?m)^hi\\Z"
#[test]
fn test_given_multiline_regex_can_insert_before_caret() {
    find_any(gs::from_regex(r"(?m)^hi\Z"), |s: &String| {
        s.contains('\n') && s.split('\n').next().is_some_and(|p| !p.is_empty())
    });
}

#[test]
fn test_does_not_left_pad_beginning_of_string_marker() {
    assert_all_examples(gs::from_regex(r"\Afoo"), |s: &String| s.starts_with("foo"));
}

#[test]
fn test_bare_caret_can_produce() {
    find_any(gs::from_regex("^"), |s: &String| !s.is_empty());
}

#[test]
fn test_bare_dollar_can_produce() {
    find_any(gs::from_regex("$"), |s: &String| !s.is_empty());
}

#[test]
fn test_shared_union() {
    check_can_generate_examples(gs::from_regex(".|."));
}

#[test]
fn test_issue_992_regression() {
    // Verbose regex: whitespace and # comments are stripped
    check_can_generate_examples(gs::from_regex(
        r"(?x)\d +  # the integral part
            \.    # the decimal point
            \d *  # some fractional digits",
    ));
}

// test_fullmatch_generates_example: parametrized; bytes variants omitted.
#[test]
fn test_fullmatch_generates_example_literal() {
    find_any(gs::from_regex("a").fullmatch(true), |s: &String| s == "a");
}

#[test]
fn test_fullmatch_generates_example_charset() {
    find_any(gs::from_regex("[Aa]").fullmatch(true), |s: &String| {
        s == "A"
    });
}

#[test]
fn test_fullmatch_generates_example_star() {
    find_any(gs::from_regex("[ab]*").fullmatch(true), |s: &String| {
        s == "abb"
    });
}

#[test]
fn test_fullmatch_generates_example_ignorecase_charset() {
    // Uses a larger max_attempts because the target "aBb" has roughly 0.15%
    // per-draw probability ([ab]* with IGNORECASE expands to 4 chars, length-3
    // is ~10% of draws, specific ordering is 1/64). 1000 attempts gives only a
    // ~78% pass rate; 10_000 pushes this above 99.999%.
    FindAny::new(
        gs::from_regex(r"(?i)[ab]*").fullmatch(true),
        |s: &String| s == "aBb",
    )
    .max_attempts(10_000)
    .run();
}

#[test]
fn test_fullmatch_generates_example_ignorecase_single() {
    find_any(gs::from_regex(r"(?i)[ab]").fullmatch(true), |s: &String| {
        s == "A"
    });
}

// test_fullmatch_matches: parametrized; bytes and compiled-with-flags variants adapted.
#[test]
fn test_fullmatch_matches_empty() {
    assert_all_examples(gs::from_regex("").fullmatch(true), |s: &String| {
        Regex::new(r"\A\z").unwrap().is_match(s)
    });
}

#[test]
fn test_fullmatch_matches_comment() {
    assert_all_examples(
        gs::from_regex("(?#comment)").fullmatch(true),
        |s: &String| Regex::new(r"\A\z").unwrap().is_match(s),
    );
}

#[test]
fn test_fullmatch_matches_literal_a() {
    assert_all_examples(gs::from_regex("a").fullmatch(true), |s: &String| {
        Regex::new(r"\Aa\z").unwrap().is_match(s)
    });
}

#[test]
fn test_fullmatch_matches_charset_aa() {
    assert_all_examples(gs::from_regex("[Aa]").fullmatch(true), |s: &String| {
        Regex::new(r"\A[Aa]\z").unwrap().is_match(s)
    });
}

#[test]
fn test_fullmatch_matches_star() {
    assert_all_examples(gs::from_regex("[ab]*").fullmatch(true), |s: &String| {
        Regex::new(r"\A[ab]*\z").unwrap().is_match(s)
    });
}

#[test]
fn test_fullmatch_matches_ignorecase_star() {
    let re = Regex::new(r"(?i)\A[ab]*\z").unwrap();
    assert_all_examples(
        gs::from_regex(r"(?i)[ab]*").fullmatch(true),
        move |s: &String| re.is_match(s),
    );
}

#[test]
fn test_fullmatch_matches_ignorecase_single() {
    let re = Regex::new(r"(?i)\A[ab]\z").unwrap();
    assert_all_examples(
        gs::from_regex(r"(?i)[ab]").fullmatch(true),
        move |s: &String| re.is_match(s),
    );
}

// test_fullmatch_must_be_bool: omitted — hegel-rust fullmatch() takes bool, not Option<bool>.

// test_issue_1786_regression: re.compile("\\\\", flags=re.IGNORECASE) == r"(?i)\\"
#[test]
fn test_issue_1786_regression() {
    check_can_generate_examples(gs::from_regex(r"(?i)\\"));
}

#[test]
fn test_sets_allow_multichar_output_in_ignorecase_mode() {
    // \u{130} is İ (Latin Capital Letter I With Dot Above); with IGNORECASE,
    // it folds to the multi-character sequence "i\u{307}".
    find_any(gs::from_regex("(?i)[\u{0130}_]"), |s: &String| {
        s.chars().count() > 1
    });
}

// test_internals_can_disable_newline_from_dollar_for_jsonschema: uses regex_strategy (internal).
// test_can_pass_union_for_alphabet: uses union alphabet type not supported by hegel-rust's API.
// test_regex_output_should_print_as_string: output formatting test (subprocess).
