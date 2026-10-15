//! Tests for `src/native/lark/lower.rs`: that groups and repetitions
//! become nonterminals with the right productions and provenance.

use super::*;
use crate::native::lark::builder::{NoImports, load_grammar};
use crate::native::lark::compile::compile;
use crate::native::lark::lexsim::Lexer;
use crate::native::lark::parse::{Parser, Tree};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec;

fn lowered(text: &str) -> (Grammar, Lowered) {
    let built = load_grammar(text, &NoImports).unwrap();
    let grammar = compile(&built, &["start"]).unwrap();
    let lowered = lower(&grammar);
    (grammar, lowered)
}

fn rhs_lens(lowered: &Lowered, lhs: NtId) -> Vec<usize> {
    lowered.by_lhs[lhs]
        .iter()
        .map(|&p| lowered.productions[p].rhs.len())
        .collect()
}

#[test]
fn rules_keep_their_alternatives() {
    let (grammar, lowered) = lowered("start: a \"x\" | b\na: \"a\"\nb: \"b\"\n");
    let start = grammar.rule_id("start").unwrap();
    let a = grammar.rule_id("a").unwrap();
    let b = grammar.rule_id("b").unwrap();
    let x = grammar.terminal_id("X").unwrap();
    assert_eq!(lowered.by_lhs.len(), grammar.rules.len());
    let prods: Vec<&Production> = lowered.by_lhs[start]
        .iter()
        .map(|&p| &lowered.productions[p])
        .collect();
    assert_eq!(
        prods,
        vec![
            &Production {
                lhs: start,
                rhs: vec![Symbol::Nonterminal(a), Symbol::Terminal(x)],
                provenance: Provenance::Rule { id: start, alt: 0 },
            },
            &Production {
                lhs: start,
                rhs: vec![Symbol::Nonterminal(b)],
                provenance: Provenance::Rule { id: start, alt: 1 },
            },
        ]
    );
}

#[test]
fn groups_become_nonterminals_with_one_production_per_alternative() {
    let (grammar, lowered) = lowered("start: (\"a\" | \"b\" \"c\") \"d\"\n");
    let start = grammar.rule_id("start").unwrap();
    let prod = &lowered.productions[lowered.by_lhs[start][0]];
    let Symbol::Nonterminal(group) = prod.rhs[0] else {
        panic!("{prod:?}");
    };
    assert_eq!(group, grammar.rules.len());
    let alts: Vec<(Provenance, usize)> = lowered.by_lhs[group]
        .iter()
        .map(|&p| {
            (
                lowered.productions[p].provenance,
                lowered.productions[p].rhs.len(),
            )
        })
        .collect();
    assert_eq!(
        alts,
        vec![
            (Provenance::Group { alt: 0 }, 1),
            (Provenance::Group { alt: 1 }, 2)
        ]
    );
}

fn repeat_nonterminals(grammar: &Grammar, lowered: &Lowered) -> Vec<NtId> {
    let start = grammar.rule_id("start").unwrap();
    let prod = &lowered.productions[lowered.by_lhs[start][0]];
    prod.rhs
        .iter()
        .map(|s| match s {
            Symbol::Nonterminal(nt) => *nt,
            Symbol::Terminal(_) => panic!("{prod:?}"),
        })
        .collect()
}

#[test]
fn narrow_bounded_repeats_unroll_to_each_count() {
    let (grammar, lowered) = lowered("start: \"a\"~1..3 \"b\"? \"c\"~3 \"d\"~2..3\n");
    let repeats = repeat_nonterminals(&grammar, &lowered);
    assert_eq!(rhs_lens(&lowered, repeats[0]), vec![1, 2, 3]);
    assert_eq!(rhs_lens(&lowered, repeats[1]), vec![0, 1]);
    assert_eq!(rhs_lens(&lowered, repeats[2]), vec![3]);
    assert_eq!(rhs_lens(&lowered, repeats[3]), vec![2, 3]);
    for nt in repeats {
        for &p in &lowered.by_lhs[nt] {
            assert_eq!(lowered.productions[p].provenance, Provenance::Repeat);
        }
    }
}

/// How many derivations of each length of a part has: the number of
/// ways it derives that many items.
fn derivation_counts(lowered: &Lowered, symbol: Symbol) -> BTreeMap<usize, usize> {
    let Symbol::Nonterminal(nt) = symbol else {
        return BTreeMap::from([(1, 1)]);
    };
    let mut counts = BTreeMap::new();
    for &p in &lowered.by_lhs[nt] {
        let mut lengths = BTreeMap::from([(0, 1)]);
        for &s in &lowered.productions[p].rhs {
            let mut next = BTreeMap::new();
            for (a, n) in &lengths {
                for (b, m) in derivation_counts(lowered, s) {
                    *next.entry(a + b).or_insert(0) += n * m;
                }
            }
            lengths = next;
        }
        for (length, n) in lengths {
            *counts.entry(length).or_insert(0) += n;
        }
    }
    counts
}

fn finished(lowering: Lowering) -> Lowered {
    let productions = lowering.productions;
    let mut by_lhs = vec![Vec::new(); lowering.nonterminals];
    for (id, production) in productions.iter().enumerate() {
        by_lhs[production.lhs].push(id);
    }
    Lowered {
        productions,
        by_lhs,
    }
}

#[test]
fn exact_parts_derive_their_count_in_one_way() {
    for n in 0..=70u32 {
        let mut lowering = Lowering {
            productions: Vec::new(),
            nonterminals: 0,
        };
        let mut parts = Parts::new(Symbol::Terminal(0));
        let rhs = parts.exact(&mut lowering, n);
        assert_eq!(rhs.len(), n.count_ones() as usize);
        let lhs = lowering.fresh();
        lowering.part(lhs, rhs);
        let lowered = finished(lowering);
        assert_eq!(
            derivation_counts(&lowered, Symbol::Nonterminal(lhs)),
            BTreeMap::from([(n as usize, 1)]),
            "{n}"
        );
        assert!(lowered.productions.len() <= 8, "{n}");
    }
}

#[test]
fn up_to_parts_derive_every_count_to_their_bound_in_one_way() {
    for d in 1..=70u32 {
        let mut lowering = Lowering {
            productions: Vec::new(),
            nonterminals: 0,
        };
        let mut parts = Parts::new(Symbol::Terminal(0));
        let part = parts.up_to(&mut lowering, d);
        let lowered = finished(lowering);
        let expected: BTreeMap<usize, usize> = (0..=d as usize).map(|n| (n, 1)).collect();
        assert_eq!(derivation_counts(&lowered, part), expected, "{d}");
        assert!(lowered.productions.len() <= 40, "{d}");
    }
}

#[test]
fn wide_bounded_repeats_lower_to_logarithmically_many_productions() {
    let (grammar, lowered) =
        lowered("start: \"a\"~0..1048576 \"b\"~1000..1048576 \"c\"~1048576 \"d\"~5..1048575\n");
    assert!(
        lowered.productions.len() < 400,
        "{}",
        lowered.productions.len()
    );
    let symbols: usize = lowered.productions.iter().map(|p| p.rhs.len()).sum();
    assert!(symbols < 600, "{symbols}");
    let repeats = repeat_nonterminals(&grammar, &lowered);
    for nt in repeats {
        assert_eq!(lowered.by_lhs[nt].len(), 1);
        assert_eq!(
            lowered.productions[lowered.by_lhs[nt][0]].provenance,
            Provenance::Repeat
        );
    }
    for p in &lowered.productions {
        assert!(p.rhs.len() <= 21, "{p:?}");
    }
}

#[test]
fn an_exact_wide_repeat_is_a_sum_of_powers_of_two() {
    let (grammar, lowered) = lowered("start: \"a\"~100\n");
    let top = repeat_nonterminals(&grammar, &lowered)[0];
    let prod = &lowered.productions[lowered.by_lhs[top][0]];
    assert_eq!(prod.rhs.len(), 3);
    assert_eq!(
        derivation_counts(&lowered, Symbol::Nonterminal(top)),
        BTreeMap::from([(100, 1)])
    );
}

#[test]
fn factorised_repeats_parse_like_unrolled_ones() {
    let (grammar, _) = lowered("start: \"a\"~2..20 \"b\"~0..40\n");
    let lexer = Lexer::new(&grammar.terminals);
    let parser = Parser::new(&grammar, grammar.start[0]);
    let accepts = |text: &str| parser.accepts(&lexer, &text.chars().collect::<Vec<char>>());
    assert!(!accepts("a"));
    for n in 2..=20 {
        assert!(accepts(&"a".repeat(n)), "{n}");
    }
    assert!(!accepts(&"a".repeat(21)));
    for n in 0..=40 {
        assert!(accepts(&format!("aa{}", "b".repeat(n))), "{n}");
    }
    assert!(!accepts(&format!("aa{}", "b".repeat(41))));
    let tree = parser
        .parse(
            &lexer,
            &format!("{}{}", "a".repeat(13), "b".repeat(37))
                .chars()
                .collect::<Vec<char>>(),
        )
        .unwrap();
    let Tree::Rule { children, .. } = &tree else {
        panic!("{tree:?}");
    };
    assert!(matches!(&children[0], Tree::Repeat { children } if children.len() == 13));
    assert!(matches!(&children[1], Tree::Repeat { children } if children.len() == 37));
}

#[test]
fn unbounded_repeats_add_one_item_at_a_time_after_their_minimum() {
    let (grammar, lowered) = lowered("start: \"a\"* (\"b\" | \"c\")+\n");
    let repeats = repeat_nonterminals(&grammar, &lowered);
    let a = grammar.terminal_id("A").unwrap();
    let star = &lowered.productions[lowered.by_lhs[repeats[0]][0]];
    assert_eq!(star.provenance, Provenance::Repeat);
    let [Symbol::Nonterminal(any)] = star.rhs[..] else {
        panic!("{star:?}");
    };
    assert_eq!(
        lowered.by_lhs[any]
            .iter()
            .map(|&p| &lowered.productions[p])
            .collect::<Vec<_>>(),
        vec![
            &Production {
                lhs: any,
                rhs: vec![],
                provenance: Provenance::RepeatPart,
            },
            &Production {
                lhs: any,
                rhs: vec![Symbol::Nonterminal(any), Symbol::Terminal(a)],
                provenance: Provenance::RepeatPart,
            },
        ]
    );
    let plus = &lowered.productions[lowered.by_lhs[repeats[1]][0]];
    let [Symbol::Nonterminal(group), Symbol::Nonterminal(any)] = plus.rhs[..] else {
        panic!("{plus:?}");
    };
    assert_eq!(lowered.by_lhs[group].len(), 2);
    assert_eq!(lowered.by_lhs[any].len(), 2);
    let lexer = Lexer::new(&grammar.terminals);
    let parser = Parser::new(&grammar, grammar.start[0]);
    let accepts = |text: &str| parser.accepts(&lexer, &text.chars().collect::<Vec<char>>());
    assert!(accepts("aaab"));
    assert!(accepts("cbcb"));
    assert!(!accepts("aaa"));
    assert!(!accepts(""));
    let tree = parser
        .parse(
            &lexer,
            &format!("aaa{}", "b".repeat(73))
                .chars()
                .collect::<Vec<char>>(),
        )
        .unwrap();
    let Tree::Rule { children, .. } = &tree else {
        panic!("{tree:?}");
    };
    assert!(matches!(&children[0], Tree::Repeat { children } if children.len() == 3));
    assert!(matches!(&children[1], Tree::Repeat { children } if children.len() == 73));
}

#[test]
fn the_unrolling_limit_is_on_the_unrolled_symbols() {
    assert_eq!(unrolled_symbols(0, 1), 1);
    assert_eq!(unrolled_symbols(2, 3), 5);
    assert_eq!(unrolled_symbols(2, 4), 9);
    assert_eq!(unrolled_symbols(1 << 20, 1 << 20), 1 << 20);
    assert_eq!(
        unrolled_symbols(0, 1 << 20),
        (1u64 << 20) * ((1 << 20) + 1) / 2
    );
    let symbols = |lowered: &Lowered| {
        lowered
            .productions
            .iter()
            .map(|p| p.rhs.len())
            .sum::<usize>()
    };
    let (_, unrolled) = lowered("start: \"a\"~0..10\n");
    assert_eq!(unrolled.productions.len(), 1 + 11);
    assert_eq!(symbols(&unrolled), 1 + 55);
    let (_, factorised) = lowered("start: \"a\"~0..11\n");
    assert!(symbols(&factorised) < 30, "{}", symbols(&factorised));
}
