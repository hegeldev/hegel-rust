//! Embedded tests for the SRE-style matcher in `src/native/draws/regex.rs`.
//!
//! The matcher (`match_seq`) is internal: it's only exercised through
//! negative-lookahead validation, where the body shape determines which
//! arms get evaluated. End-to-end tests cover the literal-only path
//! comfortably, but the more complex arms (`Branch`, `MaxRepeat`,
//! `GroupRef`, etc.) need patterns that the generator may rarely emit
//! against. These direct-call tests pin each arm independently of the
//! generator's draw distribution.

use super::*;
use crate::native::HashMap;
use crate::native::bignum::BigInt;
use crate::native::core::ChoiceValue;
use crate::native::re::constants::{
    AtCode, ChCode, SRE_FLAG_ASCII, SRE_FLAG_DOTALL, SRE_FLAG_IGNORECASE, SRE_FLAG_MULTILINE,
};
use crate::native::re::parser::{OpCode, SetItem, SubPattern};

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn lit(cp: char) -> OpCode {
    OpCode::Literal(cp as u32)
}

fn sub(ops: Vec<OpCode>) -> SubPattern {
    SubPattern { data: ops }
}

#[test]
fn match_seq_literal_match() {
    let groups = HashMap::default();
    assert_eq!(match_seq(&[lit('a')], 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_literal_no_match() {
    let groups = HashMap::default();
    assert_eq!(match_seq(&[lit('a')], 0, &chars("b"), 0, &groups), None);
}

#[test]
fn match_seq_not_literal_match() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    assert_eq!(
        match_seq(&[OpCode::Any], 0, &chars("x"), 0, &groups),
        Some(1)
    );
}

#[test]
fn match_seq_any_does_not_match_newline_without_dotall() {
    let groups = HashMap::default();
    assert_eq!(match_seq(&[OpCode::Any], 0, &chars("\n"), 0, &groups), None);
}

#[test]
fn match_seq_any_matches_newline_with_dotall() {
    let groups = HashMap::default();
    assert_eq!(
        match_seq(&[OpCode::Any], 0, &chars("\n"), SRE_FLAG_DOTALL, &groups),
        Some(1)
    );
}

#[test]
fn match_seq_in_set_literal_match() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let items = vec![SetItem::Range('a' as u32, 'z' as u32)];
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
    let groups = HashMap::default();
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
    let groups = HashMap::default();
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

#[test]
fn match_seq_at_beginning_string() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::End)], 1, &chars("a"), 0, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::End)], 1, &chars("ab"), 0, &groups),
        None
    );
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::End)], 1, &chars("a\n"), 0, &groups),
        Some(1)
    );
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
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::Boundary)], 1, &chars("ab"), 0, &groups),
        None
    );
    assert_eq!(
        match_seq(&[OpCode::At(AtCode::Boundary)], 0, &chars("ab"), 0, &groups),
        Some(0)
    );
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

#[test]
fn match_seq_branch_first_arm_matches() {
    let groups = HashMap::default();
    let ops = vec![OpCode::Branch(vec![
        sub(vec![lit('a')]),
        sub(vec![lit('b')]),
    ])];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_branch_second_arm_matches() {
    let groups = HashMap::default();
    let ops = vec![OpCode::Branch(vec![
        sub(vec![lit('a')]),
        sub(vec![lit('b')]),
    ])];
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), Some(1));
}

#[test]
fn match_seq_branch_no_match() {
    let groups = HashMap::default();
    let ops = vec![OpCode::Branch(vec![
        sub(vec![lit('a')]),
        sub(vec![lit('b')]),
    ])];
    assert_eq!(match_seq(&ops, 0, &chars("c"), 0, &groups), None);
}

#[test]
fn match_seq_subpattern() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::Subpattern {
        group: None,
        add_flags: SRE_FLAG_IGNORECASE,
        del_flags: 0,
        p: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("A"), 0, &groups), Some(1));
}

#[test]
fn match_seq_atomic_group() {
    let groups = HashMap::default();
    let ops = vec![OpCode::AtomicGroup(sub(vec![lit('a')]))];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), None);
}

#[test]
fn match_seq_groupref_match() {
    let mut groups = HashMap::default();
    groups.insert(1, "ab".to_string());
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("ab"), 0, &groups), Some(2));
}

#[test]
fn match_seq_groupref_too_short() {
    let mut groups = HashMap::default();
    groups.insert(1, "abc".to_string());
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("ab"), 0, &groups), None);
}

#[test]
fn match_seq_groupref_mismatched() {
    let mut groups = HashMap::default();
    groups.insert(1, "ab".to_string());
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("xy"), 0, &groups), None);
}

#[test]
fn match_seq_groupref_unset() {
    let groups = HashMap::default();
    let ops = vec![OpCode::GroupRef(1)];
    assert_eq!(match_seq(&ops, 0, &chars("ab"), 0, &groups), None);
}

#[test]
fn match_seq_groupref_exists_yes_arm() {
    let mut groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::GroupRefExists {
        cond_group: 1,
        yes: sub(vec![lit('a')]),
        no: Some(sub(vec![lit('b')])),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), Some(1));
}

#[test]
fn match_seq_groupref_exists_no_arm_missing() {
    let groups = HashMap::default();
    let ops = vec![OpCode::GroupRefExists {
        cond_group: 1,
        yes: sub(vec![lit('a')]),
        no: None,
    }];
    assert_eq!(match_seq(&ops, 0, &chars(""), 0, &groups), Some(0));
}

#[test]
fn match_seq_positive_lookaround_match() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::Assert {
        direction: 1,
        p: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), None);
}

#[test]
fn match_seq_negative_lookaround_match() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::AssertNot {
        direction: 1,
        p: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), None);
}

#[test]
fn match_seq_failure_never_matches() {
    let groups = HashMap::default();
    assert_eq!(
        match_seq(&[OpCode::Failure], 0, &chars(""), 0, &groups),
        None
    );
}

#[test]
fn match_seq_max_repeat_unbounded() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::MaxRepeat {
        min: 2,
        max: 3,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aaaa"), 0, &groups), Some(3));
}

#[test]
fn match_seq_max_repeat_min_unsatisfied() {
    let groups = HashMap::default();
    let ops = vec![OpCode::MaxRepeat {
        min: 3,
        max: 5,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aa"), 0, &groups), None);
}

#[test]
fn match_seq_max_repeat_with_trailing() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::MinRepeat {
        min: 1,
        max: 2,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
}

#[test]
fn match_seq_min_repeat_no_match() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::MinRepeat {
        min: 3,
        max: 5,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aa"), 0, &groups), None);
}

#[test]
fn match_seq_min_repeat_max_exhausted() {
    let groups = HashMap::default();
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
    let groups = HashMap::default();
    let ops = vec![OpCode::PossessiveRepeat {
        min: 0,
        max: u32::MAX,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aaa"), 0, &groups), Some(3));
}

#[test]
fn match_seq_possessive_repeat_bounded() {
    let groups = HashMap::default();
    let ops = vec![OpCode::PossessiveRepeat {
        min: 0,
        max: 2,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("aaa"), 0, &groups), Some(2));
}

#[test]
fn match_seq_possessive_repeat_min_unsatisfied() {
    let groups = HashMap::default();
    let ops = vec![OpCode::PossessiveRepeat {
        min: 3,
        max: 5,
        item: sub(vec![lit('a')]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), None);
}

#[test]
fn match_seq_min_repeat_zero_width_item_at_min() {
    let groups = HashMap::default();
    let ops = vec![OpCode::MinRepeat {
        min: 1,
        max: u32::MAX,
        item: sub(vec![]),
    }];
    assert_eq!(match_seq(&ops, 0, &chars(""), 0, &groups), Some(0));
}

#[test]
fn match_seq_min_repeat_zero_width_item_after_min() {
    let groups = HashMap::default();
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

#[test]
fn build_in_set_ascii_flag_keeps_nonascii_positive_literal() {
    let items = vec![SetItem::Literal('a' as u32), SetItem::Literal(0xFF)];
    let out = build_in_set(&items, SRE_FLAG_ASCII, &None).unwrap();
    assert_eq!(out, vec!['a', '\u{FF}']);
}

#[test]
fn build_in_set_ascii_ignorecase_folds_only_ascii_letters() {
    let items = vec![SetItem::Literal('à' as u32), SetItem::Literal('a' as u32)];
    let out = build_in_set(&items, SRE_FLAG_ASCII | SRE_FLAG_IGNORECASE, &None).unwrap();
    assert_eq!(out, vec!['à', 'a', 'A']);
}

#[test]
fn build_in_set_ignorecase_includes_every_case_equal_char() {
    let items = vec![SetItem::Literal('k' as u32)];
    let out = build_in_set(&items, SRE_FLAG_IGNORECASE, &None).unwrap();
    assert_eq!(out, vec!['k', 'K', '\u{212A}']);
}

#[test]
fn build_in_set_ignorecase_range_includes_case_equivalents_of_its_members() {
    let items = vec![SetItem::Range('a' as u32, 'z' as u32)];
    let out = build_in_set(&items, SRE_FLAG_IGNORECASE, &None).unwrap();
    for extra in ['K', '\u{212A}', 'ſ', '\u{131}', '\u{130}'] {
        assert!(out.contains(&extra), "{extra:?} missing from {out:?}");
    }
    assert_eq!(out.len(), 26 * 2 + 4);
}

#[test]
fn build_in_set_ascii_categories_are_ascii_only() {
    let items = vec![SetItem::Category(ChCode::Word)];
    let alphabet =
        IntervalSet::new(vec![('0' as u32, 'z' as u32), ('é' as u32, 'é' as u32)]).unwrap();
    let out = build_in_set(&items, SRE_FLAG_ASCII, &Some(alphabet.clone())).unwrap();
    assert!(
        out.iter().all(|c| c.is_ascii_alphanumeric() || *c == '_'),
        "{out:?}"
    );
    let out = build_in_set(&items, 0, &Some(alphabet)).unwrap();
    assert!(out.contains(&'é'));
}

#[test]
fn build_in_set_negated_ignorecase_excludes_case_equal_chars() {
    let items = vec![SetItem::Negate, SetItem::Literal('k' as u32)];
    let alphabet = IntervalSet::new(vec![
        ('K' as u32, 'K' as u32),
        ('k' as u32, 'k' as u32),
        ('x' as u32, 'x' as u32),
        (0x212A, 0x212A),
    ])
    .unwrap();
    let out = build_in_set(&items, SRE_FLAG_IGNORECASE, &Some(alphabet)).unwrap();
    assert_eq!(out, vec!['x']);
}

#[test]
fn build_in_set_positive_enumeration_agrees_with_the_matcher() {
    let cases: Vec<(Vec<SetItem>, u32)> = vec![
        (vec![SetItem::Literal('k' as u32)], SRE_FLAG_IGNORECASE),
        (vec![SetItem::Literal('ß' as u32)], SRE_FLAG_IGNORECASE),
        (
            vec![SetItem::Literal('\u{130}' as u32)],
            SRE_FLAG_IGNORECASE,
        ),
        (
            vec![SetItem::Range('a' as u32, 'z' as u32)],
            SRE_FLAG_IGNORECASE,
        ),
        (vec![SetItem::Range(0x3b1, 0x3c9)], SRE_FLAG_IGNORECASE),
        (vec![SetItem::Range(0x3b1, 0x3c9)], 0),
        (
            vec![
                SetItem::Range('À' as u32, 'Ö' as u32),
                SetItem::Literal('a' as u32),
            ],
            SRE_FLAG_IGNORECASE | SRE_FLAG_ASCII,
        ),
        (
            vec![
                SetItem::Category(ChCode::Digit),
                SetItem::Literal('x' as u32),
            ],
            SRE_FLAG_IGNORECASE | SRE_FLAG_ASCII,
        ),
    ];
    for (items, flags) in cases {
        let enumerated: HashSet<char> = build_in_set(&items, flags, &None)
            .unwrap()
            .into_iter()
            .filter(|c| (*c as u32) <= 0xFFFF)
            .collect();
        let matched: HashSet<char> = gather_chars(&None, |c| char_matches_set(&items, c, flags))
            .into_iter()
            .collect();
        assert_eq!(enumerated, matched, "{items:?} with flags {flags}");
    }
}

#[test]
fn in_category_uses_ascii_definitions_under_the_ascii_flag() {
    assert!(in_category('٣', ChCode::Digit, false));
    assert!(!in_category('٣', ChCode::Digit, true));
    assert!(in_category('٣', ChCode::NotDigit, true));
    assert!(in_category('\x1c', ChCode::Space, false));
    assert!(!in_category('\x1c', ChCode::Space, true));
    assert!(in_category('\x1c', ChCode::NotSpace, true));
    assert!(in_category(' ', ChCode::Space, true));
    assert!(in_category('é', ChCode::Word, false));
    assert!(!in_category('é', ChCode::Word, true));
    assert!(in_category('é', ChCode::NotWord, true));
    assert!(in_category('_', ChCode::Word, true));
}

#[test]
fn build_in_set_alphabet_drops_disallowed_positive_literal() {
    let items = vec![SetItem::Literal('a' as u32), SetItem::Literal('b' as u32)];
    let alphabet = IntervalSet::new(vec![('a' as u32, 'a' as u32)]).unwrap();
    let out = build_in_set(&items, 0, &Some(alphabet)).unwrap();
    assert_eq!(out, vec!['a']);
}

#[test]
fn build_in_set_negated_ascii_flag_keeps_nonascii() {
    let items = vec![SetItem::Negate, SetItem::Literal('a' as u32)];
    let alphabet = IntervalSet::new(vec![(b' ' as u32, 0x100)]).unwrap();
    let out = build_in_set(&items, SRE_FLAG_ASCII, &Some(alphabet)).unwrap();
    assert!(out.iter().all(|c| *c != 'a'));
    assert!(out.contains(&'\u{100}'));
}

#[test]
fn generate_op_literal_with_no_candidate_in_alphabet_marks_invalid() {
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(0))], None, None);
    let caches = Caches::default();
    let mut state = ignorecase_state(&caches);
    let alphabet = Some(IntervalSet::new(vec![('b' as u32, 'b' as u32)]).unwrap());
    let mut out = String::new();
    let result = generate_op(&mut ntc, &lit('a'), &mut state, &alphabet, &mut out);
    assert!(result.is_err());
    assert_eq!(ntc.status(), Some(Status::Invalid));
}

#[test]
fn generate_op_ignorecase_literal_takes_the_case_the_alphabet_allows() {
    let mut ntc = NativeTestCase::for_choices(&[], None, None);
    let caches = Caches::default();
    let mut state = ignorecase_state(&caches);
    let alphabet = Some(IntervalSet::new(vec![('A' as u32, 'A' as u32)]).unwrap());
    let mut out = String::new();
    generate_op(&mut ntc, &lit('a'), &mut state, &alphabet, &mut out).unwrap();
    assert_eq!(out, "A");
}

#[test]
fn compile_rejects_unparseable_patterns() {
    let err = CompiledRegex::compile("(unclosed", None).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("invalid regex pattern"));
    assert!(CompiledRegex::compile("a+b?", None).is_ok());
}

fn ignorecase_state(caches: &Caches) -> GenState<'_> {
    GenState {
        groups: HashMap::default(),
        flags: SRE_FLAG_IGNORECASE,
        fullmatch: false,
        pending_anchors: Vec::new(),
        pending_asserts: Vec::new(),
        pending_lookaheads: Vec::new(),
        needs_whole_match: false,
        caches,
    }
}

#[test]
fn generate_op_ignorecase_eszett_emits_only_the_chars_python_folds_it_to() {
    use crate::native::rng::EngineRng;
    let caches = Caches::default();
    let mut seen = HashSet::default();
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let mut state = ignorecase_state(&caches);
        let mut out = String::new();
        generate_op(&mut ntc, &lit('ß'), &mut state, &None, &mut out).unwrap();
        assert!(
            out == "ß" || out == "\u{1E9E}",
            "seed {seed} emitted a non-matching case variant {out:?}"
        );
        seen.insert(out);
    }
    assert_eq!(seen.len(), 2, "saw {seen:?}");
}

#[test]
fn generate_op_ignorecase_plain_letter_emits_both_cases() {
    use crate::native::rng::EngineRng;
    let mut seen = HashSet::default();
    let caches = Caches::default();
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let mut state = ignorecase_state(&caches);
        let mut out = String::new();
        generate_op(&mut ntc, &lit('a'), &mut state, &None, &mut out).unwrap();
        seen.insert(out);
    }
    assert!(seen.contains("a") && seen.contains("A"), "saw {seen:?}");
}

#[test]
fn generate_op_ignorecase_not_literal_excludes_every_case_equal_char() {
    use crate::native::rng::EngineRng;
    let alphabet = Some(
        IntervalSet::new(vec![
            ('I' as u32, 'I' as u32),
            ('i' as u32, 'i' as u32),
            ('x' as u32, 'x' as u32),
            (0x130, 0x130),
            (0x131, 0x131),
            (0x307, 0x307),
        ])
        .unwrap(),
    );
    let caches = Caches::default();
    let mut seen = HashSet::default();
    for seed in 0..100 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let mut state = ignorecase_state(&caches);
        let mut out = String::new();
        generate_op(
            &mut ntc,
            &OpCode::NotLiteral('İ' as u32),
            &mut state,
            &alphabet,
            &mut out,
        )
        .unwrap();
        assert!(
            out == "x" || out == "\u{307}",
            "seed {seed} emitted a case-equal char {out:?}"
        );
        seen.insert(out);
    }
    assert_eq!(seen.len(), 2, "saw {seen:?}");
}

#[test]
fn generate_regex_ascii_flag_generates_explicit_nonascii_chars() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("(?a)[Ï-İ]", None).unwrap();
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let s = generate_regex(&mut ntc, &re, true).unwrap();
        let c = s.chars().next().unwrap();
        assert!(s.chars().count() == 1 && ('Ï'..='İ').contains(&c), "{s:?}");
    }
    let re = CompiledRegex::compile("(?a)[^\\x00-\\xff]", None).unwrap();
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let s = generate_regex(&mut ntc, &re, true).unwrap();
        assert!(s.chars().all(|c| c as u32 > 0xFF), "{s:?}");
    }
}

#[test]
fn generate_regex_ascii_ignorecase_folds_only_ascii_letters() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("(?ai)À", None).unwrap();
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert_eq!(generate_regex(&mut ntc, &re, true).unwrap(), "À");
    }
    let re = CompiledRegex::compile("(?ai)a", None).unwrap();
    let mut seen = HashSet::default();
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        seen.insert(generate_regex(&mut ntc, &re, true).unwrap());
    }
    assert!(seen.contains("a") && seen.contains("A"), "saw {seen:?}");
}

#[test]
fn generate_regex_skips_a_repeat_whose_body_the_alphabet_cannot_supply() {
    use crate::native::rng::EngineRng;
    let alphabet = Some(IntervalSet::new(vec![(0, 127)]).unwrap());
    let re = CompiledRegex::compile("(?-i:Ā)*k", alphabet).unwrap();
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert_eq!(
            generate_regex(&mut ntc, &re, true).unwrap(),
            "k",
            "seed {seed}"
        );
    }
}

#[test]
fn generate_regex_required_repeat_of_an_unproducible_body_is_rejected() {
    use crate::native::rng::EngineRng;
    let alphabet = Some(IntervalSet::new(vec![(0, 127)]).unwrap());
    let re = CompiledRegex::compile("Ā+k", alphabet).unwrap();
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert!(generate_regex(&mut ntc, &re, true).is_err(), "seed {seed}");
    }
}

#[test]
fn generate_regex_branch_picks_only_producible_alternatives() {
    use crate::native::rng::EngineRng;
    let alphabet = Some(IntervalSet::new(vec![(0, 127)]).unwrap());
    let re = CompiledRegex::compile("Ā|b|Ē", alphabet.clone()).unwrap();
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert_eq!(
            generate_regex(&mut ntc, &re, true).unwrap(),
            "b",
            "seed {seed}"
        );
    }
    let re = CompiledRegex::compile("Āx|Ēy", alphabet).unwrap();
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert!(generate_regex(&mut ntc, &re, true).is_err(), "seed {seed}");
    }
}

#[test]
fn generate_regex_empty_negative_lookahead_is_rejected() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("(?!)", None).unwrap();
    for seed in 0..5 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert!(generate_regex(&mut ntc, &re, true).is_err(), "seed {seed}");
        assert_eq!(ntc.status(), Some(Status::Invalid));
    }
}

#[test]
fn generate_regex_pattern_with_a_nul_character_matches_itself() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("a\0b", None).unwrap();
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    assert_eq!(generate_regex(&mut ntc, &re, true).unwrap(), "a\0b");
}

#[test]
fn producible_op_covers_every_op_kind() {
    let alphabet = Some(IntervalSet::new(vec![('a' as u32, 'z' as u32)]).unwrap());
    let ok = |op: &OpCode| producible_op(op, 0, &alphabet, &Caches::default()).unwrap();
    let dead = sub(vec![lit('Ā')]);
    let live = sub(vec![lit('a')]);
    assert!(ok(&lit('a')) && !ok(&lit('Ā')));
    assert!(ok(&OpCode::NotLiteral('a' as u32)));
    assert!(ok(&OpCode::Any));
    assert!(ok(&OpCode::In(vec![SetItem::Literal('a' as u32)])));
    assert!(!ok(&OpCode::In(vec![SetItem::Literal('Ā' as u32)])));
    assert!(ok(&OpCode::At(AtCode::Boundary)));
    assert!(ok(&OpCode::GroupRef(1)));
    assert!(ok(&OpCode::AssertNot {
        direction: 1,
        p: dead.clone()
    }));
    assert!(!ok(&OpCode::Failure));
    assert!(ok(&OpCode::Branch(vec![dead.clone(), live.clone()])));
    assert!(!ok(&OpCode::Branch(vec![dead.clone()])));
    assert!(!ok(&OpCode::Subpattern {
        group: None,
        add_flags: 0,
        del_flags: 0,
        p: dead.clone()
    }));
    assert!(ok(&OpCode::GroupRefExists {
        cond_group: 1,
        yes: dead.clone(),
        no: None
    }));
    assert!(ok(&OpCode::GroupRefExists {
        cond_group: 1,
        yes: dead.clone(),
        no: Some(live.clone())
    }));
    assert!(!ok(&OpCode::GroupRefExists {
        cond_group: 1,
        yes: dead.clone(),
        no: Some(dead.clone())
    }));
    assert!(!ok(&OpCode::Assert {
        direction: 1,
        p: dead.clone()
    }));
    assert!(ok(&OpCode::AtomicGroup(live.clone())));
    assert!(ok(&OpCode::MaxRepeat {
        min: 0,
        max: 3,
        item: dead.clone()
    }));
    assert!(!ok(&OpCode::MinRepeat {
        min: 1,
        max: 3,
        item: dead.clone()
    }));
    assert!(ok(&OpCode::PossessiveRepeat {
        min: 1,
        max: 3,
        item: live.clone()
    }));
    let caches = Caches::default();
    assert!(producible_sub(&SubPattern::new(), 0, &alphabet, &caches).unwrap());
    assert!(!producible_sub(&dead, 0, &alphabet, &caches).unwrap());
    assert!(!producible_sub(&dead, 0, &alphabet, &caches).unwrap());
}

#[test]
fn match_seq_literal_ignorecase_uses_python_folding() {
    let groups = HashMap::default();
    let ic = SRE_FLAG_IGNORECASE;
    assert_eq!(
        match_seq(&[lit('k')], 0, &chars("\u{212A}"), ic, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&[lit('ß')], 0, &chars("\u{1E9E}"), ic, &groups),
        Some(1)
    );
    assert_eq!(match_seq(&[lit('ſ')], 0, &chars("S"), ic, &groups), Some(1));
    assert_eq!(match_seq(&[lit('1')], 0, &chars("1"), ic, &groups), Some(1));
    assert_eq!(
        match_seq(&[lit('k')], 0, &chars("\u{212A}"), 0, &groups),
        None
    );
    assert_eq!(
        match_seq(&[lit('À')], 0, &chars("à"), ic | SRE_FLAG_ASCII, &groups),
        None
    );
    assert_eq!(
        match_seq(
            &[OpCode::NotLiteral('k' as u32)],
            0,
            &chars("\u{212A}"),
            ic,
            &groups
        ),
        None
    );
}

#[test]
fn match_seq_groupref_ignorecase_compares_lowercase_forms() {
    let mut groups = HashMap::default();
    groups.insert(1, "s".to_string());
    let ops = [OpCode::GroupRef(1)];
    assert_eq!(
        match_seq(&ops, 0, &chars("S"), SRE_FLAG_IGNORECASE, &groups),
        Some(1)
    );
    assert_eq!(
        match_seq(&ops, 0, &chars("ſ"), SRE_FLAG_IGNORECASE, &groups),
        None
    );
    assert_eq!(match_seq(&ops, 0, &chars("S"), 0, &groups), None);
}

#[test]
fn match_seq_in_set_ascii_flag_keeps_explicit_chars_and_narrows_categories() {
    let groups = HashMap::default();
    let range = [OpCode::In(vec![SetItem::Range('Ï' as u32, 'İ' as u32)])];
    assert_eq!(
        match_seq(&range, 0, &chars("Ð"), SRE_FLAG_ASCII, &groups),
        Some(1)
    );
    let word = [OpCode::In(vec![SetItem::Category(ChCode::Word)])];
    assert_eq!(
        match_seq(&word, 0, &chars("é"), SRE_FLAG_ASCII, &groups),
        None
    );
    assert_eq!(match_seq(&word, 0, &chars("é"), 0, &groups), Some(1));
    let negated = [OpCode::In(vec![
        SetItem::Negate,
        SetItem::Literal('k' as u32),
    ])];
    assert_eq!(
        match_seq(
            &negated,
            0,
            &chars("\u{212A}"),
            SRE_FLAG_IGNORECASE,
            &groups
        ),
        None
    );
    assert_eq!(
        match_seq(
            &negated,
            0,
            &chars("\u{212A}"),
            SRE_FLAG_IGNORECASE | SRE_FLAG_ASCII,
            &groups
        ),
        Some(1)
    );
}

#[test]
fn at_matches_word_boundary_is_ascii_under_the_ascii_flag() {
    let cs = chars("éx");
    assert!(at_matches(&AtCode::Boundary, &cs, 1, SRE_FLAG_ASCII));
    assert!(!at_matches(&AtCode::Boundary, &cs, 1, 0));
    assert!(at_matches(&AtCode::NonBoundary, &cs, 1, 0));
}

#[test]
fn generate_regex_handles_huge_character_class_ranges() {
    use crate::native::rng::EngineRng;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0)).unwrap();
    let re = CompiledRegex::compile("[\\x20-\\U0010FFFF]", None).unwrap();
    let s = generate_regex(&mut ntc, &re, false).unwrap();
    assert!(!s.is_empty());
}

#[test]
fn generate_regex_word_boundaries_hold_in_the_final_string() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile(r"\bfoo\b", None).unwrap();
    let mut produced = 0;
    for seed in 0..300 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let Ok(s) = generate_regex(&mut ntc, &re, false) else {
            continue;
        };
        produced += 1;
        let cs = chars(&s);
        let matched = (0..cs.len().saturating_sub(2)).any(|i| {
            cs[i..i + 3] == ['f', 'o', 'o']
                && is_word_boundary(&cs, i, false)
                && is_word_boundary(&cs, i + 3, false)
        });
        assert!(matched, "seed {seed}: {s:?} contains no \\bfoo\\b match");
    }
    assert!(produced > 0, "every draw was rejected");
}

#[test]
fn generate_regex_end_anchor_inside_branch_holds_in_the_final_string() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("foo$|bar", None).unwrap();
    let mut produced = 0;
    for seed in 0..300 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let Ok(s) = generate_regex(&mut ntc, &re, false) else {
            continue;
        };
        produced += 1;
        assert!(
            s.contains("bar") || s.ends_with("foo") || s.ends_with("foo\n"),
            "seed {seed}: {s:?} matches neither branch"
        );
    }
    assert!(produced > 0, "every draw was rejected");
}

#[test]
fn generate_regex_fullmatch_lookahead_does_not_emit_the_assertion_body() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("(?=a)ab", None).unwrap();
    let mut produced = 0;
    for seed in 0..100 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let Ok(s) = generate_regex(&mut ntc, &re, true) else {
            continue;
        };
        produced += 1;
        assert_eq!(s, "ab", "seed {seed}: {s:?} is not a fullmatch of (?=a)ab");
    }
    assert!(produced > 0, "every draw was rejected");
}

#[test]
fn generate_regex_unsatisfiable_possessive_pattern_never_yields_a_wrong_string() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("a*+a", None).unwrap();
    for seed in 0..100 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        for fullmatch in [false, true] {
            let mut ntc2 = NativeTestCase::new_random(EngineRng::seeded(seed + 1000)).unwrap();
            let ntc_ref = if fullmatch { &mut ntc2 } else { &mut ntc };
            if let Ok(s) = generate_regex(ntc_ref, &re, fullmatch) {
                panic!(
                    "seed {seed} fullmatch={fullmatch}: a*+a produced {s:?}, but no string matches it"
                );
            }
        }
    }
}

#[test]
fn generate_regex_ignorecase_negated_class_excludes_every_case_equal_char() {
    use crate::native::rng::EngineRng;
    let alphabet = Some(
        IntervalSet::new(vec![
            ('I' as u32, 'I' as u32),
            ('a' as u32, 'a' as u32),
            ('i' as u32, 'i' as u32),
            ('x' as u32, 'x' as u32),
            (0x130, 0x131),
            (0x307, 0x307),
        ])
        .unwrap(),
    );
    let re = CompiledRegex::compile("(?i)[^\u{130}a]", alphabet).unwrap();
    let mut seen = HashSet::default();
    for seed in 0..100 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let s = generate_regex(&mut ntc, &re, true).unwrap();
        assert!(
            s == "x" || s == "\u{307}",
            "seed {seed}: {s:?} is case-equal to an excluded char"
        );
        seen.insert(s);
    }
    assert_eq!(seen.len(), 2, "saw {seen:?}");
}

#[test]
fn match_seq_max_repeat_counts_zero_width_iterations_toward_min() {
    let groups = HashMap::default();
    let optional_a = sub(vec![OpCode::MaxRepeat {
        min: 0,
        max: 1,
        item: sub(vec![lit('a')]),
    }]);
    let pattern = [OpCode::MaxRepeat {
        min: 3,
        max: 3,
        item: optional_a,
    }];
    assert_eq!(match_seq(&pattern, 0, &chars(""), 0, &groups), Some(0));
    assert_eq!(match_seq(&pattern, 0, &chars("aa"), 0, &groups), Some(2));
}

#[test]
fn match_seq_min_repeat_counts_zero_width_iterations_toward_min() {
    let groups = HashMap::default();
    let optional_a = sub(vec![OpCode::MaxRepeat {
        min: 0,
        max: 1,
        item: sub(vec![lit('a')]),
    }]);
    let pattern = [OpCode::MinRepeat {
        min: 3,
        max: 3,
        item: optional_a,
    }];
    assert_eq!(match_seq(&pattern, 0, &chars(""), 0, &groups), Some(0));
}

#[test]
fn generate_regex_fullmatch_lookbehind_is_validated_against_the_final_string() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("(?<=a)b", None).unwrap();
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert!(
            generate_regex(&mut ntc, &re, true).is_err(),
            "a fixed-width lookbehind can never hold at the start of a fullmatch"
        );
    }

    let re = CompiledRegex::compile("(?<=a*)b", None).unwrap();
    let mut produced = 0;
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        if let Ok(s) = generate_regex(&mut ntc, &re, true) {
            produced += 1;
            assert_eq!(s, "b");
        }
    }
    assert!(produced > 0, "an empty-matching lookbehind body must pass");
}

#[test]
fn generate_regex_end_anchor_with_no_newline_in_alphabet_does_not_pad() {
    use crate::native::rng::EngineRng;
    let alphabet = Some(IntervalSet::new(vec![('a' as u32, 'b' as u32)]).unwrap());
    let re = CompiledRegex::compile("a$", alphabet).unwrap();
    let mut produced = 0;
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        let Ok(s) = generate_regex(&mut ntc, &re, false) else {
            continue;
        };
        produced += 1;
        assert!(s.ends_with('a'), "no newline available to pad with: {s:?}");
    }
    assert!(produced > 0);
}

#[test]
fn generate_regex_multiline_caret_in_the_middle_generates_nothing() {
    use crate::native::rng::EngineRng;
    let re = CompiledRegex::compile("(?m)a^b", None).unwrap();
    for seed in 0..20 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
        assert!(
            generate_regex(&mut ntc, &re, false).is_err(),
            "a mid-pattern ^ preceded by a literal can never match"
        );
    }
}

#[test]
fn match_seq_possessive_repeat_counts_zero_width_iterations_toward_min() {
    let groups = HashMap::default();
    let optional_a = sub(vec![OpCode::MaxRepeat {
        min: 0,
        max: 1,
        item: sub(vec![lit('a')]),
    }]);
    let pattern = [OpCode::PossessiveRepeat {
        min: 3,
        max: 3,
        item: optional_a,
    }];
    assert_eq!(match_seq(&pattern, 0, &chars(""), 0, &groups), Some(0));
    assert_eq!(match_seq(&pattern, 0, &chars("aa"), 0, &groups), Some(2));
}

#[test]
fn codepoint_to_char_reports_surrogates_as_internal_errors() {
    assert_eq!(codepoint_to_char('a' as u32).unwrap(), 'a');
    assert!(codepoint_to_char(0xD800).is_err());
}
