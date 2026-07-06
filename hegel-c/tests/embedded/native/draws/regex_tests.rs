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
    assert_eq!(match_seq(&ops, 0, &chars("A"), 0, &groups), Some(1));
}

#[test]
fn match_seq_atomic_group() {
    let groups = HashMap::new();
    let ops = vec![OpCode::AtomicGroup(sub(vec![lit('a')]))];
    assert_eq!(match_seq(&ops, 0, &chars("a"), 0, &groups), Some(1));
    assert_eq!(match_seq(&ops, 0, &chars("b"), 0, &groups), None);
}

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
    let groups = HashMap::new();
    let ops = vec![OpCode::GroupRefExists {
        cond_group: 1,
        yes: sub(vec![lit('a')]),
        no: None,
    }];
    assert_eq!(match_seq(&ops, 0, &chars(""), 0, &groups), Some(0));
}

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

#[test]
fn build_in_set_ascii_only_drops_nonascii_positive_literal() {
    let items = vec![SetItem::Literal('a' as u32), SetItem::Literal(0xFF)];
    let out = build_in_set(&items, SRE_FLAG_ASCII, &None);
    assert_eq!(out, vec!['a']);
}

#[test]
fn build_in_set_alphabet_drops_disallowed_positive_literal() {
    let items = vec![SetItem::Literal('a' as u32), SetItem::Literal('b' as u32)];
    let alphabet = IntervalSet::new(vec![('a' as u32, 'a' as u32)]);
    let out = build_in_set(&items, 0, &Some(alphabet));
    assert_eq!(out, vec!['a']);
}

#[test]
fn build_in_set_negated_ascii_only_excludes_nonascii() {
    let items = vec![SetItem::Negate, SetItem::Literal('a' as u32)];
    let alphabet = IntervalSet::new(vec![(b' ' as u32, 0x100)]);
    let out = build_in_set(&items, SRE_FLAG_ASCII, &Some(alphabet));
    assert!(out.iter().all(|c| (*c as u32) < 128 && *c != 'a'));
}

#[test]
fn generate_op_ignorecase_literal_outside_alphabet_marks_invalid() {
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(0))], None, None);
    let cache = Mutex::new(HashMap::new());
    let mut state = ignorecase_state(&cache);
    let alphabet = Some(IntervalSet::new(vec![('A' as u32, 'A' as u32)]));
    let mut out = String::new();
    let result = generate_op(&mut ntc, &lit('a'), &mut state, &alphabet, &mut out);
    assert!(result.is_err());
    assert_eq!(ntc.status(), Some(Status::Invalid));
}

#[test]
fn compile_rejects_unparseable_patterns() {
    let err = CompiledRegex::compile("(unclosed", None).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("invalid regex pattern"));
    assert!(CompiledRegex::compile("a+b?", None).is_ok());
}

fn ignorecase_state(cache: &Mutex<HashMap<InKey, Arc<[char]>>>) -> GenState<'_> {
    GenState {
        groups: HashMap::new(),
        flags: SRE_FLAG_IGNORECASE,
        pending_lookaheads: Vec::new(),
        in_cache: cache,
    }
}

#[test]
fn generate_op_ignorecase_eszett_never_emits_truncated_uppercase() {
    use crate::native::rng::EngineRng;
    let cache = Mutex::new(HashMap::new());
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut state = ignorecase_state(&cache);
        let mut out = String::new();
        generate_op(&mut ntc, &lit('ß'), &mut state, &None, &mut out).unwrap();
        assert_eq!(out, "ß", "seed {seed} emitted a non-matching case variant");
    }
}

#[test]
fn generate_op_ignorecase_plain_letter_emits_both_cases() {
    use crate::native::rng::EngineRng;
    let mut seen = std::collections::HashSet::new();
    let cache = Mutex::new(HashMap::new());
    for seed in 0..50 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut state = ignorecase_state(&cache);
        let mut out = String::new();
        generate_op(&mut ntc, &lit('a'), &mut state, &None, &mut out).unwrap();
        seen.insert(out);
    }
    assert!(seen.contains("a") && seen.contains("A"), "saw {seen:?}");
}

#[test]
fn generate_op_ignorecase_not_literal_blacklists_swapcase_fixpoint() {
    use crate::native::rng::EngineRng;
    let alphabet = Some(IntervalSet::new(vec![
        ('I' as u32, 'I' as u32),
        ('i' as u32, 'i' as u32),
        ('x' as u32, 'x' as u32),
        (0x130, 0x130),
        (0x307, 0x307),
    ]));
    let cache = Mutex::new(HashMap::new());
    for seed in 0..100 {
        let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed));
        let mut state = ignorecase_state(&cache);
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
fn generate_regex_handles_huge_character_class_ranges() {
    use crate::native::rng::EngineRng;
    let mut ntc = NativeTestCase::new_random(EngineRng::seeded(0));
    let re = CompiledRegex::compile("[\\x20-\\U0010FFFF]", None).unwrap();
    let s = generate_regex(&mut ntc, &re, false).unwrap();
    assert!(!s.is_empty());
}
