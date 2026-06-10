//! Embedded tests for the SRE-style matcher in `src/native/schema/regex.rs`.
//!
//! The matcher (`match_seq`) is internal: it's only exercised through
//! negative-lookahead validation, where the body shape determines which
//! arms get evaluated. End-to-end tests cover the literal-only path
//! comfortably, but the more complex arms (`Branch`, `MaxRepeat`,
//! `GroupRef`, etc.) need patterns that the generator may rarely emit
//! against. These direct-call tests pin each arm independently of the
//! generator's draw distribution.

use super::*;
use crate::native::bignum::BigInt;
use crate::native::core::ChoiceValue;
use crate::native::re::constants::{
    AtCode, ChCode, SRE_FLAG_DOTALL, SRE_FLAG_IGNORECASE, SRE_FLAG_MULTILINE,
};
use crate::native::re::parser::{OpCode, SetItem, SubPattern};
use std::collections::HashMap;

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn lit(cp: char) -> OpCode {
    OpCode::Literal(cp as u32)
}

fn sub(ops: Vec<OpCode>) -> SubPattern {
    SubPattern { data: ops }
}

// ----- Literal / NotLiteral / Any -----

#[test]
fn match_seq_literal_match() {
    let groups = HashMap::new();
    assert_eq!(match_seq(&[lit('a')], 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_literal_no_match() {
    let groups = HashMap::new();
    assert_eq!(match_seq(&[lit('a')], 0, &chars("b"), 0, &groups), None);
}

#[test]
fn match_seq_not_literal_match() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(
            &[OpCode::NotLiteral('a' as u32)],
            0,
            &chars("b"),
            0,
            &groups
        ),
        Some(1)
    );
}

#[test]
fn match_seq_not_literal_no_match() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(
            &[OpCode::NotLiteral('a' as u32)],
            0,
            &chars("a"),
            0,
            &groups
        ),
        None
    );
}

#[test]
fn match_seq_any_matches_non_newline() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(&[OpCode::Any], 0, &chars("x"), 0, &groups),
        Some(1)
    );
}

#[test]
fn match_seq_any_does_not_match_newline_without_dotall() {
    let groups = HashMap::new();
    assert_eq!(match_seq(&[OpCode::Any], 0, &chars("\n"), 0, &groups), None);
}

#[test]
fn match_seq_any_matches_newline_with_dotall() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(&[OpCode::Any], 0, &chars("\n"), SRE_FLAG_DOTALL, &groups),
        Some(1)
    );
}

// ----- In / char_matches_set / SetItem -----

#[test]
fn match_seq_in_set_literal_match() {
    let groups = HashMap::new();
    let items = vec![SetItem::Literal('a' as u32), SetItem::Literal('b' as u32)];
    assert_eq!(
        match_seq(&[OpCode::In(items.clone())], 0, &chars("a"), 0, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&[OpCode::In(items)], 0, &chars("c"), 0, &groups),
        None
    );
}

#[test]
fn match_seq_in_set_range_match() {
    let groups = HashMap::new();
    let items = vec![SetItem::Range('a' as u32, 'z' as u32)];
    assert_eq!(
        match_seq(&[OpCode::In(items.clone())], 0, &chars("m"), 0, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&[OpCode::In(items)], 0, &chars("A"), 0, &groups),
        None
    );
}

#[test]
fn match_seq_in_set_range_ignorecase() {
    let groups = HashMap::new();
    let items = vec![SetItem::Range('a' as u32, 'z' as u32)];
    // With IGNORECASE, the matcher folds the input char's case before
    // checking the range, so 'M' matches the lowercase a-z range.
    assert_eq!(
        match_seq(
            &[OpCode::In(items)],
            0,
            &chars("M"),
            SRE_FLAG_IGNORECASE,
            &groups
        ),
        Some(1)
    );
}

#[test]
fn match_seq_in_set_category_match() {
    let groups = HashMap::new();
    let items = vec![SetItem::Category(ChCode::Digit)];
    assert_eq!(
        match_seq(&[OpCode::In(items.clone())], 0, &chars("5"), 0, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&[OpCode::In(items)], 0, &chars("a"), 0, &groups),
        None
    );
}

#[test]
fn match_seq_in_set_negated() {
    let groups = HashMap::new();
    let items = vec![SetItem::Negate, SetItem::Literal('a' as u32)];
    assert_eq!(
        match_seq(&[OpCode::In(items.clone())], 0, &chars("b"), 0, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&[OpCode::In(items)], 0, &chars("a"), 0, &groups),
        None
    );
}

// ----- At / at_matches -----

#[test]
fn match_seq_at_beginning_string() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(
            &[OpCode::At(AtCode::BeginningString)],
            0,
            &chars(""),
            0,
            &groups
        ),
        Some(0)
    );
    assert_eq!(
        match_seq(
            &[OpCode::At(AtCode::BeginningString)],
            1,
            &chars("ab"),
            0,
            &groups
        ),
        None
    );
}

#[test]
fn match_seq_at_beginning() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::Beginning)], 0, &chars("a"), 0, &groups),
        Some(0)
    );
    assert_eq!(
        match_seq(
            &[OpCode::At(AtCode::Beginning)],
            1,
            &chars("ab"),
            0,
            &groups
        ),
        None
    );
    // MULTILINE: matches at start of line (after newline).
    assert_eq!(
        match_seq(
            &[OpCode::At(AtCode::Beginning)],
            1,
            &chars("\na"),
            SRE_FLAG_MULTILINE,
            &groups
        ),
        Some(1)
    );
}

#[test]
fn match_seq_at_end() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::End)], 1, &chars("a"), 0, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::End)], 1, &chars("ab"), 0, &groups),
        None
    );
    // End matches just before a trailing newline.
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::End)], 1, &chars("a\n"), 0, &groups),
        Some(1)
    );
    // MULTILINE: matches at end of any line.
    assert_eq!(
        match_seq(
            &[OpCode::At(AtCode::End)],
            1,
            &chars("a\nb"),
            SRE_FLAG_MULTILINE,
            &groups
        ),
        Some(1)
    );
}

#[test]
fn match_seq_at_end_string() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::EndString)], 1, &chars("a"), 0, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(
            &[OpCode::At(AtCode::EndString)],
            1,
            &chars("ab"),
            0,
            &groups
        ),
        None
    );
}

#[test]
fn match_seq_at_word_boundary() {
    let groups = HashMap::new();
    // Word boundary at position 1 in "ab": between two word chars → false.
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::Boundary)], 1, &chars("ab"), 0, &groups),
        None
    );
    // Word boundary at start of "ab" (transition from non-word to word).
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::Boundary)], 0, &chars("ab"), 0, &groups),
        Some(0)
    );
    // Non-boundary: between two word chars → matches.
    assert_eq!(
        match_seq(
            &[OpCode::At(AtCode::NonBoundary)],
            1,
            &chars("ab"),
            0,
            &groups
        ),
        Some(1)
    );
}

// ----- Branch -----

#[test]
fn match_seq_branch_first_arm_matches() {
    let groups = HashMap::new();
    let ops = vec![OpCode::Branch(vec![
        sub(vec![lit('a')]),
        sub(vec![lit('b')]),
    ])];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_branch_second_arm_matches() {
    let groups = HashMap::new();
    let ops = vec![OpCode::Branch(vec![
        sub(vec![lit('a')]),
        sub(vec![lit('b')]),
    ])];
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), Some(1));
}

#[test]
fn match_seq_branch_no_match() {
    let groups = HashMap::new();
    let ops = vec![OpCode::Branch(vec![
        sub(vec![lit('a')]),
        sub(vec![lit('b')]),
    ])];
    assert_eq!(match_seq(&ops, 0, &chars("c"), 0, &groups), None);
}

// ----- Subpattern / AtomicGroup -----

#[test]
fn match_seq_subpattern() {
    let groups = HashMap::new();
    let ops = vec![OpCode::Subpattern {
        group: Some(1),
        add_flags: 0,
        del_flags: 0,
        p: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_subpattern_inline_flags() {
    let groups = HashMap::new();
    let ops = vec![OpCode::Subpattern {
        group: None,
        add_flags: SRE_FLAG_IGNORECASE,
        del_flags: 0,
        p: sub(vec![lit('a')]),
    }];
    // Inner IGNORECASE folds 'A' to 'a' for the literal compare.
    assert_eq!(match_seq(&ops, 0, &chars("A"), 0, &groups), Some(1));
}

#[test]
fn match_seq_atomic_group() {
    let groups = HashMap::new();
    let ops = vec![OpCode::AtomicGroup(sub(vec![lit('a')]))];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), None);
}

// ----- GroupRef -----

#[test]
fn match_seq_groupref_match() {
    let mut groups = HashMap::new();
    groups.insert(1, "ab".to_string());
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("ab"), 0, &groups), Some(2));
}

#[test]
fn match_seq_groupref_too_short() {
    let mut groups = HashMap::new();
    groups.insert(1, "abc".to_string());
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("ab"), 0, &groups), None);
}

#[test]
fn match_seq_groupref_mismatched() {
    let mut groups = HashMap::new();
    groups.insert(1, "ab".to_string());
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("xy"), 0, &groups), None);
}

#[test]
fn match_seq_groupref_unset() {
    let groups = HashMap::new();
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("ab"), 0, &groups), None);
}

// ----- GroupRefExists -----

#[test]
fn match_seq_groupref_exists_yes_arm() {
    let mut groups = HashMap::new();
    groups.insert(1, "x".to_string());
    let ops = vec![OpCode::GroupRefExists {
        cond_group: 1,
        yes: sub(vec![lit('a')]),
        no: Some(sub(vec![lit('b')])),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_groupref_exists_no_arm() {
    let groups = HashMap::new();
    let ops = vec![OpCode::GroupRefExists {
        cond_group: 1,
        yes: sub(vec![lit('a')]),
        no: Some(sub(vec![lit('b')])),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), Some(1));
}

#[test]
fn match_seq_groupref_exists_no_arm_missing() {
    // When the no-branch is absent and the group is unset, the conditional
    // expands to nothing — match_seq just continues with `rest`.
    let groups = HashMap::new();
    let ops = vec![OpCode::GroupRefExists {
        cond_group: 1,
        yes: sub(vec![lit('a')]),
        no: None,
    }];
    assert_eq!(match_seq(&ops, 0, &chars(""), 0, &groups), Some(0));
}

// ----- Assert / AssertNot / Failure -----

#[test]
fn match_seq_positive_lookaround_match() {
    let groups = HashMap::new();
    let ops = vec![
        OpCode::Assert {
            direction: 1,
            p: sub(vec![lit('a')]),
        },
        lit('a'),
    ];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_positive_lookaround_no_match() {
    let groups = HashMap::new();
    let ops = vec![OpCode::Assert {
        direction: 1,
        p: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), None);
}

#[test]
fn match_seq_negative_lookaround_match() {
    let groups = HashMap::new();
    let ops = vec![
        OpCode::AssertNot {
            direction: 1,
            p: sub(vec![lit('a')]),
        },
        lit('b'),
    ];
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), Some(1));
}

#[test]
fn match_seq_negative_lookaround_blocks() {
    let groups = HashMap::new();
    let ops = vec![OpCode::AssertNot {
        direction: 1,
        p: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), None);
}

#[test]
fn match_seq_failure_never_matches() {
    let groups = HashMap::new();
    assert_eq!(
        match_seq(&[OpCode::Failure], 0, &chars(""), 0, &groups),
        None
    );
}

// ----- MaxRepeat / MinRepeat / PossessiveRepeat -----

#[test]
fn match_seq_max_repeat_unbounded() {
    let groups = HashMap::new();
    let ops = vec![OpCode::MaxRepeat {
        min: 0,
        max: u32::MAX,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aaa"), 0, &groups), Some(3));
    assert_eq!(match_seq(&ops, 0, &chars(""), 0, &groups), Some(0));
}

#[test]
fn match_seq_max_repeat_bounded() {
    let groups = HashMap::new();
    // a{2,3} on "aaaa": should match up to 3.
    let ops = vec![OpCode::MaxRepeat {
        min: 2,
        max: 3,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aaaa"), 0, &groups), Some(3));
}

#[test]
fn match_seq_max_repeat_min_unsatisfied() {
    let groups = HashMap::new();
    let ops = vec![OpCode::MaxRepeat {
        min: 3,
        max: 5,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aa"), 0, &groups), None);
}

#[test]
fn match_seq_max_repeat_with_trailing() {
    // a{1,3}b — greedy match backs off until the trailing 'b' matches.
    let groups = HashMap::new();
    let ops = vec![
        OpCode::MaxRepeat {
            min: 1,
            max: 3,
            item: sub(vec![lit('a')]),
        },
        lit('b'),
    ];
    assert_eq!(match_seq(&ops, 0, &chars("aaab"), 0, &groups), Some(4));
}

#[test]
fn match_seq_min_repeat_lazy() {
    let groups = HashMap::new();
    // a*?b on "aaab": lazy match expands as needed.
    let ops = vec![
        OpCode::MinRepeat {
            min: 0,
            max: u32::MAX,
            item: sub(vec![lit('a')]),
        },
        lit('b'),
    ];
    assert_eq!(match_seq(&ops, 0, &chars("aaab"), 0, &groups), Some(4));
}

#[test]
fn match_seq_min_repeat_bounded() {
    let groups = HashMap::new();
    let ops = vec![OpCode::MinRepeat {
        min: 1,
        max: 2,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_min_repeat_no_match() {
    let groups = HashMap::new();
    let ops = vec![
        OpCode::MinRepeat {
            min: 0,
            max: u32::MAX,
            item: sub(vec![lit('a')]),
        },
        lit('b'),
    ];
    assert_eq!(match_seq(&ops, 0, &chars("aaa"), 0, &groups), None);
}

#[test]
fn match_seq_min_repeat_min_unsatisfied() {
    let groups = HashMap::new();
    let ops = vec![OpCode::MinRepeat {
        min: 3,
        max: 5,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aa"), 0, &groups), None);
}

#[test]
fn match_seq_min_repeat_max_exhausted() {
    // a{,2}?b on "aaab": lazy expansion stops at max=2, then 'b' must match
    // but chars[2]='a' so this fails.
    let groups = HashMap::new();
    let ops = vec![
        OpCode::MinRepeat {
            min: 0,
            max: 2,
            item: sub(vec![lit('a')]),
        },
        lit('b'),
    ];
    assert_eq!(match_seq(&ops, 0, &chars("aaab"), 0, &groups), None);
}

#[test]
fn match_seq_possessive_repeat() {
    let groups = HashMap::new();
    let ops = vec![OpCode::PossessiveRepeat {
        min: 0,
        max: u32::MAX,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aaa"), 0, &groups), Some(3));
}

#[test]
fn match_seq_possessive_repeat_bounded() {
    let groups = HashMap::new();
    let ops = vec![OpCode::PossessiveRepeat {
        min: 0,
        max: 2,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aaa"), 0, &groups), Some(2));
}

#[test]
fn match_seq_possessive_repeat_min_unsatisfied() {
    let groups = HashMap::new();
    let ops = vec![OpCode::PossessiveRepeat {
        min: 3,
        max: 5,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), None);
}

#[test]
fn match_seq_min_repeat_zero_width_item_at_min() {
    // An empty repetition body matches zero-width, which the MinRepeat
    // arm rejects via the `next <= cur` guard so we don't loop forever.
    let groups = HashMap::new();
    let ops = vec![OpCode::MinRepeat {
        min: 1,
        max: u32::MAX,
        item: sub(vec![]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars(""), 0, &groups), None);
}

#[test]
fn match_seq_min_repeat_zero_width_item_after_min() {
    // After satisfying min, the post-min loop bails out on the
    // zero-width body via the same `next <= cur` guard.
    let groups = HashMap::new();
    let ops = vec![
        OpCode::MinRepeat {
            min: 0,
            max: u32::MAX,
            item: sub(vec![]),
        },
        lit('a'),
    ];
    assert_eq!(match_seq(&ops, 0, &chars(""), 0, &groups), None);
}

// ----- Direct tests for build_in_set's ASCII-only / alphabet filters -----

#[test]
fn build_in_set_ascii_only_drops_nonascii_positive_literal() {
    // Positive class `[aÿ]` with ASCII flag: the non-ASCII literal is
    // filtered out by the ascii_only check (line 518 `continue`).
    let items = vec![SetItem::Literal('a' as u32), SetItem::Literal(0xFF)];
    let out = build_in_set(&items, SRE_FLAG_ASCII, &None);
    assert_eq!(out, vec!['a']);
}

#[test]
fn build_in_set_alphabet_drops_disallowed_positive_literal() {
    // Positive class `[ab]` with alphabet allowing only 'a': 'b' is
    // filtered out (line 521 `continue`).
    let items = vec![SetItem::Literal('a' as u32), SetItem::Literal('b' as u32)];
    let alphabet = IntervalSet::new(vec![('a' as u32, 'a' as u32)]);
    let out = build_in_set(&items, 0, &Some(alphabet));
    assert_eq!(out, vec!['a']);
}

#[test]
fn build_in_set_negated_ascii_only_excludes_nonascii() {
    // Negated class `[^a]` with ASCII flag and alphabet covering some
    // non-ASCII codepoints: the predicate filters out non-ASCII
    // candidates (line 546 `return false`).
    let items = vec![SetItem::Negate, SetItem::Literal('a' as u32)];
    let alphabet = IntervalSet::new(vec![(b' ' as u32, 0x100)]);
    let out = build_in_set(&items, SRE_FLAG_ASCII, &Some(alphabet));
    // Every char in the output must be ASCII and not 'a'.
    assert!(out.iter().all(|c| (*c as u32) < 128 && *c != 'a'));
}

// ----- generate_op: IGNORECASE literal rejected by the alphabet -----

#[test]
fn generate_op_ignorecase_literal_outside_alphabet_marks_invalid() {
    // `(?i)a`: the literal 'a' swapcases to 'A', so `generate_op` draws
    // `which` to choose between the two cases. With an alphabet that only
    // allows 'A', forcing `which = 0` picks the lowercase 'a', which the
    // alphabet rejects, so the test case is marked invalid.
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(0))], None, None);
    let mut state = GenState {
        groups: HashMap::new(),
        flags: SRE_FLAG_IGNORECASE,
        pending_lookaheads: Vec::new(),
        in_cache: HashMap::new(),
    };
    let alphabet = Some(IntervalSet::new(vec![('A' as u32, 'A' as u32)]));
    let mut out = String::new();
    let result = generate_op(&mut ntc, &lit('a'), &mut state, &alphabet, &mut out);
    assert!(result.is_err());
    assert_eq!(ntc.status, Some(Status::Invalid));
}

// ----- interpret_regex: caller-reachable InvalidArgument paths -----

#[test]
fn interpret_regex_missing_pattern_is_invalid_argument() {
    use crate::cbor_utils::cbor_map;
    let mut ntc = NativeTestCase::for_choices(&[], None, None);
    let schema = cbor_map! { "type" => "regex" };
    let err = interpret_regex(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("pattern"));
}

#[test]
fn interpret_regex_unparseable_pattern_is_invalid_argument() {
    use crate::cbor_utils::cbor_map;
    let mut ntc = NativeTestCase::for_choices(&[], None, None);
    // An unbalanced group is a parse error in the Python-compatible parser.
    let schema = cbor_map! { "type" => "regex", "pattern" => "(unclosed" };
    let err = interpret_regex(&mut ntc, &schema).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("invalid regex pattern"));
}

// ----- IGNORECASE swapcase safety -----

fn ignorecase_state() -> GenState {
    GenState {
        groups: HashMap::new(),
        flags: SRE_FLAG_IGNORECASE,
        pending_lookaheads: Vec::new(),
        in_cache: HashMap::new(),
    }
}

#[test]
fn generate_op_ignorecase_eszett_never_emits_truncated_uppercase() {
    // 'ß'.to_uppercase() is "SS"; truncating it to 'S' generated strings the
    // pattern does not match (re.match(r"(?i)ß", "S") is None — Hypothesis
    // guards this with an explicit re.match check). With no usable
    // single-char swap the literal must always emit 'ß'.
    use crate::native::rng::EngineRng;
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut state = ignorecase_state();
        let mut out = String::new();
        generate_op(&mut ntc, &lit('ß'), &mut state, &None, &mut out).unwrap();
        assert_eq!(out, "ß", "seed {seed} emitted a non-matching case variant");
    }
}

#[test]
fn generate_op_ignorecase_plain_letter_emits_both_cases() {
    use crate::native::rng::EngineRng;
    let mut seen = std::collections::HashSet::new();
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut state = ignorecase_state();
        let mut out = String::new();
        generate_op(&mut ntc, &lit('a'), &mut state, &None, &mut out).unwrap();
        seen.insert(out);
    }
    assert!(seen.contains("a") && seen.contains("A"), "saw {seen:?}");
}

#[test]
fn generate_op_ignorecase_not_literal_blacklists_swapcase_fixpoint() {
    // `(?i)[^İ]`: CPython's matcher treats 'i', U+0307 (combining dot), and
    // 'I' as case-equal to İ (re.fullmatch(r"[^İ]", "I", re.I) is None), so
    // none of them may be generated. The fixpoint expansion is Hypothesis's
    // fix for issue #2657. Restricting the alphabet to the case-chain plus
    // 'x' forces every draw to land on 'x'.
    use crate::native::rng::EngineRng;
    let alphabet = Some(IntervalSet::new(vec![
        ('I' as u32, 'I' as u32),
        ('i' as u32, 'i' as u32),
        ('x' as u32, 'x' as u32),
        (0x130, 0x130),
        (0x307, 0x307),
    ]));
    for seed in 0..100 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut state = ignorecase_state();
        let mut out = String::new();
        generate_op(
            &mut ntc,
            &OpCode::NotLiteral('İ' as u32),
            &mut state,
            &alphabet,
            &mut out,
        )
        .unwrap();
        assert_eq!(out, "x", "seed {seed} emitted a case-equal char");
    }
}

#[test]
fn interpret_regex_handles_huge_character_class_ranges() {
    // `[\x20-\U0010FFFF]` expands to ~1.1M codepoints; per-insert linear
    // dedup made this O(n²) (an effective hang). Consumers deduplicate with
    // a HashSet, so expansion must stay linear.
    use crate::cbor_utils::cbor_map;
    use crate::native::rng::EngineRng;
    let schema = cbor_map! {
        "type" => "regex",
        "pattern" => "[\\x20-\\U0010FFFF]"
    };
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let v = interpret_regex(&mut ntc, &schema).unwrap();
    // Strings come back as tag-91 UTF-8 bytes; one matching char suffices.
    let Value::Tag(91, inner) = v else {
        panic!("expected tag-91 string, got {v:?}")
    };
    let Value::Bytes(bytes) = *inner else {
        panic!("expected byte payload")
    };
    assert!(!bytes.is_empty());
}
