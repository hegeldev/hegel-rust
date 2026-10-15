use crate::exchange::drive_no_yield;
use crate::native::core::Constraint;
use crate::native::core::{ChoiceNode, ChoiceValue, Spans, StringChoice};
use crate::native::intervalsets::IntervalSet;
use crate::native::shrinker::{ShrinkRun, Shrinker};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

fn intervals(min: u32, max: u32) -> IntervalSet {
    IntervalSet::new(vec![(min, max)]).unwrap()
}

fn string_node(value: Vec<u32>, min_codepoint: u32, max_codepoint: u32) -> ChoiceNode {
    ChoiceNode::string(
        StringChoice {
            intervals: intervals(min_codepoint, max_codepoint).into(),
            min_size: 0,
            max_size: 32,
            constraint: None,
        },
        value,
        false,
    )
}

fn accepting_shrinker(nodes: Vec<ChoiceNode>) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => (true, nodes.to_vec(), Spans::new()),
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        nodes,
        Spans::new(),
    )
}

#[test]
fn redistribute_string_pair_moves_entire_value_when_accepted() {
    let initial = vec![
        string_node(vec![b'a' as u32, b'b' as u32, b'c' as u32], 0, 0x10FFFF),
        string_node(vec![b'd' as u32, b'e' as u32], 0, 0x10FFFF),
    ];
    let mut shrinker = accepting_shrinker(initial);
    drive_no_yield(shrinker.redistribute_string_pairs()).unwrap();
    let (a, b) = match (
        &shrinker.current_nodes[0].value(),
        &shrinker.current_nodes[1].value(),
    ) {
        (ChoiceValue::String(a), ChoiceValue::String(b)) => (a.clone(), b.clone()),
        _ => unreachable!(),
    };
    assert!(a.is_empty(), "first node not emptied: {a:?}");
    assert_eq!(
        b,
        vec![
            b'a' as u32,
            b'b' as u32,
            b'c' as u32,
            b'd' as u32,
            b'e' as u32
        ]
    );
}

#[test]
fn redistribute_string_pair_partial_move_triggers_bin_search() {
    let initial = vec![
        string_node(vec![b'a' as u32, b'b' as u32, b'c' as u32], 0, 0x10FFFF),
        string_node(vec![b'd' as u32, b'e' as u32], 0, 0x10FFFF),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let t_ok = matches!(
                    nodes.get(1).map(|n| n.value()),
                    Some(ChoiceValue::String(s)) if s.len() <= 3
                );
                (t_ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.redistribute_string_pairs()).unwrap();
    match &shrinker.current_nodes[1].value() {
        ChoiceValue::String(s) => assert!(s.len() <= 3, "t exceeded 3 cps: {s:?}"),
        _ => unreachable!(),
    }
}

#[test]
fn redistribute_string_pair_moves_several_elements_in_one_invocation() {
    let initial = vec![
        string_node("abcdefgh".chars().map(|c| c as u32).collect(), 0, 0x10FFFF),
        string_node(vec![b'z' as u32], 0, 0x10FFFF),
    ];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let s_ok = matches!(
                    nodes.first().map(|n| n.value()),
                    Some(ChoiceValue::String(s)) if !s.is_empty()
                );
                (s_ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.redistribute_string_pairs()).unwrap();
    match &shrinker.current_nodes[0].value() {
        ChoiceValue::String(s) => assert_eq!(
            s,
            &vec![b'a' as u32],
            "one invocation should move every element the predicate allows"
        ),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_strings_collapses_accepting_run_toward_simplest() {
    let initial = vec![string_node(
        vec![b'a' as u32, b'b' as u32],
        b'a' as u32,
        b'z' as u32,
    )];
    let mut shrinker = accepting_shrinker(initial);
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    let v = match &shrinker.current_nodes[0].value() {
        ChoiceValue::String(v) => v.clone(),
        _ => unreachable!(),
    };
    assert!(v.is_empty(), "expected empty shrink, got {v:?}");
}

#[test]
fn shrink_strings_duplicate_pass_bin_search_skips_after_val_eliminated() {
    let initial = vec![string_node(vec![200, 200], 0, 0x10FFFF)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let accept = matches!(
                    nodes.first().map(|n| n.value()),
                    Some(ChoiceValue::String(s))
                        if s.len() == 2
                            && s[0] == s[1]
                            && (s[0] == 100 || s[0] == 200)
                );
                (accept, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    match &shrinker.current_nodes[0].value() {
        ChoiceValue::String(s) => assert_eq!(s, &vec![100u32, 100u32]),
        _ => unreachable!(),
    }
}

#[test]
fn shrink_strings_semantic_candidate_falls_back_to_nfd_base_in_range() {
    let initial = vec![string_node(vec![0x00C0], b'A' as u32, 0x00FF)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(|run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let accept = matches!(
                    nodes.first().map(|n| n.value()),
                    Some(ChoiceValue::String(s))
                        if s.len() == 1
                            && (b'A' as u32..=b'Z' as u32).contains(&s[0])
                );
                (accept, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    match &shrinker.current_nodes[0].value() {
        ChoiceValue::String(s) => {
            assert_eq!(s.len(), 1);
            assert!((b'A' as u32..=b'Z' as u32).contains(&s[0]));
        }
        _ => unreachable!(),
    }
}

/// A truncation accepted during the bounded linear scan shortens the current
/// value below the next candidate length, which must end the scan rather
/// than slice out of bounds.
#[test]
fn shrink_strings_linear_scan_breaks_when_replace_shortens_below_target() {
    let original: Vec<u32> = "abcdefghi".chars().map(|c| c as u32).collect();
    let prefix6 = original[..6].to_vec();
    let prefix3 = original[..3].to_vec();
    let initial = vec![string_node(original.clone(), 0, 0x10FFFF)];
    let mut shrinker = Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            crate::native::shrinker::ShrinkRun::Full(nodes) => {
                let ok = matches!(
                    nodes.first().map(|n| n.value()),
                    Some(ChoiceValue::String(s))
                        if *s == original || *s == prefix6 || *s == prefix3
                );
                (ok, nodes.to_vec(), Spans::new())
            }
            crate::native::shrinker::ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        initial,
        Spans::new(),
    );
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    match &shrinker.current_nodes[0].value() {
        ChoiceValue::String(s) => {
            assert_eq!(
                s.len(),
                3,
                "expected the shortest allowed prefix, got {s:?}"
            )
        }
        _ => unreachable!(),
    }
}

fn cps(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

fn grammar_node(text: &str, simplest: &str, value: &str) -> ChoiceNode {
    let built =
        crate::native::lark::builder::load_grammar(text, &crate::native::lark::builder::NoImports)
            .unwrap();
    let grammar = crate::native::lark::compile::compile(&built, &["start"]).unwrap();
    let start = grammar.start[0];
    let constraint =
        crate::native::lark::constraint::GrammarConstraint::new(grammar, start, 32, u32::MAX, None)
            .unwrap();
    constraint
        .simplest_or_init(|| Ok::<_, crate::control::InternalError>(cps(simplest)))
        .unwrap();
    ChoiceNode::string(
        StringChoice {
            intervals: intervals(0, 0x10FFFF).into(),
            min_size: 0,
            max_size: 32,
            constraint: Some(Constraint::Grammar(constraint.into())),
        },
        cps(value),
        false,
    )
}

#[test]
fn the_string_passes_leave_a_string_under_a_grammar_alone() {
    let initial = vec![
        grammar_node("start: \"a\" \"b\"*\n", "a", "abb"),
        string_node(cps("de"), 0, 0x10FFFF),
    ];
    let mut shrinker = accepting_shrinker(initial);
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    drive_no_yield(shrinker.redistribute_string_pairs()).unwrap();
    drive_no_yield(shrinker.lower_duplicated_characters()).unwrap();
    drive_no_yield(shrinker.normalize_unicode_chars()).unwrap();
    assert_eq!(
        shrinker.current_nodes[0].value(),
        ChoiceValue::String(cps("abb"))
    );
    assert_eq!(
        shrinker.current_nodes[1].value(),
        ChoiceValue::String(cps(""))
    );
}

fn regex_node(pattern: &str, simplest: &str, value: &str) -> ChoiceNode {
    let kind = match crate::native::draws::StringSpec::regex(pattern, true, None).unwrap() {
        crate::native::draws::StringSpec::Regex { kind, .. } => kind,
        _ => unreachable!(),
    };
    let Some(Constraint::Regex(constraint)) = &kind.constraint else {
        unreachable!()
    };
    constraint
        .simplest_or_init(|| Ok::<_, ()>(cps(simplest)))
        .unwrap();
    ChoiceNode::string((*kind).clone(), cps(value), false)
}

fn least_regex_node(pattern: &str, value: &str) -> ChoiceNode {
    let spec = crate::native::draws::StringSpec::regex(pattern, true, None).unwrap();
    let crate::native::draws::StringSpec::Regex { compiled, kind, .. } = &spec else {
        unreachable!()
    };
    crate::native::draws::regex::ensure_simplest(
        &mut crate::native::draws::source::SimplestDraws::default(),
        compiled,
        true,
        kind,
    )
    .unwrap();
    ChoiceNode::string((**kind).clone(), cps(value), false)
}

fn string_shrinker(
    nodes: Vec<ChoiceNode>,
    accept: impl Fn(&[u32]) -> bool + Send + Sync + 'static,
) -> Shrinker<'static> {
    Shrinker::with_probe(
        Box::new(move |run: ShrinkRun<'_>| match run {
            ShrinkRun::Full(nodes) => {
                let ok = matches!(nodes[0].value(), ChoiceValue::String(s) if accept(&s));
                (ok, nodes.to_vec(), Spans::new())
            }
            ShrinkRun::Probe { .. } => (false, Vec::new(), Spans::new()),
        }),
        nodes,
        Spans::new(),
    )
}

fn string_of(shrinker: &Shrinker<'_>) -> String {
    match shrinker.current_nodes[0].value() {
        ChoiceValue::String(s) => s.into_iter().map(|c| char::from_u32(c).unwrap()).collect(),
        _ => unreachable!(),
    }
}

/// The passes run on a string under a regex, every candidate checked
/// against the pattern: a natural simpler character the pattern rejects
/// is passed over, and the ordinary shrink reaches the simplest.
#[test]
fn the_string_passes_shrink_a_string_under_a_regex_within_the_pattern() {
    let mut shrinker =
        accepting_shrinker(vec![regex_node("\u{101}+", "\u{101}", "\u{101}\u{101}")]);
    drive_no_yield(shrinker.normalize_unicode_chars()).unwrap();
    assert_eq!(string_of(&shrinker), "\u{101}\u{101}");
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    assert_eq!(string_of(&shrinker), "\u{101}");
}

/// The simplest string of a pattern is the least string of it, so the
/// shrink reaches `a` from `bbbb`, which no edit of `bbbb` within the
/// pattern could.
#[test]
fn a_string_under_a_regex_shrinks_to_the_least_alternative() {
    let mut shrinker = accepting_shrinker(vec![least_regex_node("(?:bbbb|a)", "bbbb")]);
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    assert_eq!(string_of(&shrinker), "a");
}

/// A run of characters the pattern only admits together is deleted at
/// once, which deleting them one at a time could not do.
#[test]
fn a_run_of_characters_a_regex_needs_together_is_deleted_at_once() {
    let mut shrinker = string_shrinker(
        vec![regex_node("(foo|barbaz)+", "foo", "foobarbazfoo")],
        |s| {
            let s: String = s.iter().map(|&c| char::from_u32(c).unwrap()).collect();
            s.contains("barbaz")
        },
    );
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    assert_eq!(string_of(&shrinker), "barbaz");
}

/// The window deletions, a few per character and each a check of the
/// whole string against the pattern, give up after `max_stall` failures
/// in a row, and carry on past that many when some succeed.
#[test]
fn window_deletions_stop_after_a_stall_and_go_on_while_they_succeed() {
    let text = "abcdefghijklmnopqrstuvwxyz";
    let calls_with = |max_stall: usize, accept: fn(&[u32]) -> bool| {
        let mut shrinker = string_shrinker(vec![regex_node("[a-z]+", "a", text)], accept);
        shrinker.max_stall = max_stall;
        drive_no_yield(shrinker.shrink_strings()).unwrap();
        (shrinker.calls, string_of(&shrinker))
    };
    let only_the_whole = |s: &[u32]| s.len() == 26;
    let (unbudgeted, kept) = calls_with(usize::MAX, only_the_whole);
    let (budgeted, kept_too) = calls_with(3, only_the_whole);
    assert_eq!((kept.len(), kept_too.len()), (26, 26));
    assert!(budgeted < unbudgeted, "{budgeted} {unbudgeted}");
    let even_up_to_z = |s: &[u32]| s.len() % 2 == 0 && s.last() == Some(&u32::from('z'));
    let (_, shrunk) = calls_with(3, even_up_to_z);
    assert_eq!(shrunk, "az");
}

/// Either end of the simplest string is copied over the string, which
/// reaches an alternative that differs in several characters at once.
#[test]
fn the_ends_of_the_simplest_string_are_copied_over_a_string_under_a_regex() {
    let mut shrinker = string_shrinker(
        vec![regex_node(
            r"(com|org)\.[a-z]+@[a-z]+\.(com|org)",
            "com.a@a.com",
            "org.aaaaaa@a.org",
        )],
        |s| s.len() >= 16,
    );
    drive_no_yield(shrinker.shrink_strings()).unwrap();
    assert_eq!(string_of(&shrinker), "com.a@aaaaaa.com");
}
