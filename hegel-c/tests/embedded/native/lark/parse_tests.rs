//! Tests for `src/native/lark/parse.rs`: acceptance against Lark's
//! Earley parser with the dynamic lexer, the derivation trees, and that
//! every string the generator draws from the fixture grammars parses.

use super::*;
use crate::native::core::NativeTestCase;
use crate::native::lark::builder::{ImportSource, NoImports, load_grammar};
use crate::native::lark::compile::compile;
use crate::native::lark::generate::{
    DEFAULT_MAX_SIZE, GrammarOptions, GrammarSpec, generate_grammar,
};
use crate::native::lark::layout::Layout;
use crate::native::rng::EngineRng;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use std::path::Path;

struct Setup {
    grammar: Grammar,
    lexer: Lexer,
    parser: Parser,
}

fn build(grammar: Grammar) -> Setup {
    let lexer = Lexer::new(&grammar.terminals);
    let parser = Parser::new(&grammar, grammar.start[0]);
    Setup {
        grammar,
        lexer,
        parser,
    }
}

fn setup(text: &str) -> Setup {
    let built = load_grammar(text, None, &NoImports).unwrap();
    build(compile(&built, &["start"]).unwrap())
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

#[test]
fn a_regex_repeat_stops_at_an_empty_iteration_after_consuming_text() {
    let s = setup("start: TOKEN \"a\"\nTOKEN: /b(?:(?=a)|.)*/\n");
    assert!(s.parser.accepts(&s.lexer, &chars("bba")));
    assert!(!s.parser.accepts(&s.lexer, &chars("baba")));
}

#[test]
fn item_limits_fail_cleanly_during_scanning_and_tree_reconstruction() {
    for (grammar, text, expected) in [
        (
            "start: \"a\" start |\n",
            "aaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaa",
        ),
        ("start: \"a\"+\n%ignore \" \"\n", " a a ", "[ ]a[ ]a[ ]"),
    ] {
        let s = setup(grammar);
        let text = chars(text);
        let mut accepted = false;
        for budget in 0..200 {
            let parser = Parser::new(&s.grammar, s.grammar.start[0]).with_item_budget(budget);
            match parser.parse(&s.lexer, &text) {
                Ok(tree) => {
                    assert_eq!(read_back(&tree, &text), expected);
                    accepted = true;
                }
                Err(_) => assert!(!accepted, "a larger budget must accept the same derivation"),
            }
        }
        assert!(accepted);
    }
}

/// The [`Tree::Terminal`] and [`Tree::Ignored`] leaves of `tree`, in text
/// order.
fn leaves(tree: &Tree) -> Vec<&Tree> {
    let mut leaves = Vec::new();
    let mut stack = vec![tree];
    while let Some(tree) = stack.pop() {
        match tree {
            Tree::Terminal { .. } | Tree::Ignored { .. } => leaves.push(tree),
            _ => stack.extend(tree.children().iter().rev()),
        }
    }
    leaves
}

/// The text a tree's leaves cover, read back from `text`, with each
/// ignored stretch in brackets.
fn read_back(tree: &Tree, text: &[char]) -> String {
    let mut out = String::new();
    for leaf in leaves(tree) {
        match leaf {
            Tree::Terminal { start, end, .. } => out.extend(&text[*start..*end]),
            Tree::Ignored { start, end, .. } => {
                out.push('[');
                out.extend(&text[*start..*end]);
                out.push(']');
            }
            _ => panic!("{leaf:?}"),
        }
    }
    out
}

fn covers(tree: &Tree, text: &[char]) -> bool {
    let mut pos = 0;
    for leaf in leaves(tree) {
        let (Tree::Terminal { start, end, .. } | Tree::Ignored { start, end, .. }) = leaf else {
            panic!("{leaf:?}");
        };
        if *start != pos || end <= start {
            return false;
        }
        pos = *end;
    }
    pos == text.len()
}

impl Setup {
    fn parse(&self, s: &str) -> Result<Tree, ParseError> {
        let text = chars(s);
        let result = self.parser.parse(&self.lexer, &text);
        if let Ok(tree) = &result {
            assert!(covers(tree, &text), "{s:?}: {tree:?}");
        }
        result
    }

    fn accepts(&self, s: &str) -> bool {
        let accepted = self.parser.accepts(&self.lexer, &chars(s));
        assert_eq!(accepted, self.parse(s).is_ok());
        accepted
    }

    fn tree(&self, s: &str) -> Tree {
        self.parse(s).unwrap()
    }

    fn rule(&self, name: &str) -> RuleId {
        self.grammar.rule_id(name).unwrap()
    }

    fn term(&self, name: &str) -> TermId {
        self.grammar.terminal_id(name).unwrap()
    }

    fn read_back(&self, s: &str) -> String {
        read_back(&self.tree(s), &chars(s))
    }
}

#[test]
fn accepts_and_rejects_simple_strings() {
    let s = setup("start: \"a\" \"b\"\n");
    assert!(s.accepts("ab"));
    assert!(!s.accepts("a"));
    assert!(!s.accepts("abc"));
    assert!(!s.accepts(""));
    assert_eq!(s.parse("a"), Err(ParseError { position: 1 }));
    assert_eq!(s.parse("abc"), Err(ParseError { position: 2 }));
    assert_eq!(s.parse("x"), Err(ParseError { position: 0 }));
}

#[test]
fn exhausted_budgets_stop_before_lexing_the_column() {
    struct Counted<'a>(&'a Lexer, core::cell::Cell<usize>);

    impl Terminals for Counted<'_> {
        type Memo = ();

        fn begin(&self, _: &[char]) {}

        fn ends(&self, memo: &mut (), id: TermId, text: &[char], pos: usize, out: &mut Vec<usize>) {
            self.1.set(self.1.get() + 1);
            self.0.ends(memo, id, text, pos, out);
        }
    }

    let s = setup("start: a\na: a a | \"a\" |\n");
    let terminals = Counted(&s.lexer, core::cell::Cell::new(0));
    assert!(s.parser.accepts(&terminals, &chars("a")));
    assert!(terminals.1.get() > 0);
    for parser in [
        Parser::new(&s.grammar, s.grammar.start[0]).with_budget(5),
        Parser::new(&s.grammar, s.grammar.start[0]).with_item_budget(5),
    ] {
        terminals.1.set(0);
        assert!(parser.parse(&terminals, &chars("a")).is_err());
        assert_eq!(terminals.1.get(), 0);
    }
}

#[test]
fn nullable_repeats_cannot_expand_an_unbounded_derivation() {
    let s = setup("start: empty ~ 65536\nempty:\n");
    let parser = Parser::new(&s.grammar, s.grammar.start[0]).with_item_budget(1000);
    assert_eq!(parser.parse(&s.lexer, &[]), Err(ParseError { position: 0 }));
}

#[test]
fn shared_nullable_rules_cannot_expand_an_unbounded_derivation() {
    let mut grammar = String::from("start: r0\n");
    for i in 0..12 {
        grammar.push_str(&format!("r{i}: r{} r{}\n", i + 1, i + 1));
    }
    grammar.push_str("r12:\n");
    let s = setup(&grammar);
    let parser = Parser::new(&s.grammar, s.grammar.start[0]).with_item_budget(1000);
    assert_eq!(parser.parse(&s.lexer, &[]), Err(ParseError { position: 0 }));
}

#[test]
fn trees_follow_the_grammar() {
    let s = setup("start: a B\na: \"x\" | \"y\"\nB: \"b\"\n");
    assert_eq!(
        s.tree("yb"),
        Tree::Rule {
            id: s.rule("start"),
            alt: 0,
            children: vec![
                Tree::Rule {
                    id: s.rule("a"),
                    alt: 1,
                    children: vec![Tree::Terminal {
                        id: s.term("Y"),
                        start: 0,
                        end: 1
                    }],
                },
                Tree::Terminal {
                    id: s.term("B"),
                    start: 1,
                    end: 2
                },
            ],
        }
    );
}

#[test]
fn nullable_rules_and_the_empty_string() {
    let s = setup("start: a b c\na: \"x\"?\nb: \"y\"*\nc: d | \"z\"\nd: e e\ne:\n");
    assert!(s.accepts(""));
    assert!(s.accepts("x"));
    assert!(s.accepts("yy"));
    assert!(s.accepts("xyyz"));
    assert!(s.accepts("z"));
    assert!(!s.accepts("zz"));
    assert!(!s.accepts("yx"));
    let tree = s.tree("");
    assert_eq!(
        tree,
        Tree::Rule {
            id: s.rule("start"),
            alt: 0,
            children: vec![
                Tree::Rule {
                    id: s.rule("a"),
                    alt: 0,
                    children: vec![Tree::Repeat { children: vec![] }],
                },
                Tree::Rule {
                    id: s.rule("b"),
                    alt: 0,
                    children: vec![Tree::Repeat { children: vec![] }],
                },
                Tree::Rule {
                    id: s.rule("c"),
                    alt: 0,
                    children: vec![Tree::Rule {
                        id: s.rule("d"),
                        alt: 0,
                        children: vec![
                            Tree::Rule {
                                id: s.rule("e"),
                                alt: 0,
                                children: vec![]
                            },
                            Tree::Rule {
                                id: s.rule("e"),
                                alt: 0,
                                children: vec![]
                            },
                        ],
                    }],
                },
            ],
        }
    );
    assert!(leaves(&tree).is_empty());
}

#[test]
fn nullable_items_complete_whichever_order_they_are_predicted_in() {
    let s = setup("start: a a \"x\"\na: b | \"y\"\nb: c\nc: a?\n");
    assert!(s.accepts("x"));
    assert!(s.accepts("yx"));
    assert!(s.accepts("yyx"));
    assert!(!s.accepts("yyyx"));
}

#[test]
fn groups_and_counted_repeats() {
    let s = setup("start: (\"a\" | \"b\" \"c\")~2..3 \"d\"? (\"e\"~2)*\n");
    assert!(!s.accepts("a"));
    assert!(s.accepts("aa"));
    assert!(s.accepts("abc"));
    assert!(s.accepts("bcbca"));
    assert!(!s.accepts("bcbcaa"));
    assert!(s.accepts("aad"));
    assert!(s.accepts("aadeeee"));
    assert!(!s.accepts("aadeee"));
    assert!(s.accepts("aaeeee"));
    let tree = s.tree("abcdee");
    let Tree::Rule { children, .. } = &tree else {
        panic!("{tree:?}");
    };
    assert_eq!(children.len(), 3);
    let Tree::Repeat { children: reps } = &children[0] else {
        panic!("{tree:?}");
    };
    assert_eq!(reps.len(), 2);
    assert!(matches!(&reps[0], Tree::Group { alt: 0, children } if children.len() == 1));
    assert!(matches!(&reps[1], Tree::Group { alt: 1, children } if children.len() == 2));
    assert!(matches!(&children[1], Tree::Repeat { children } if children.len() == 1));
    let Tree::Repeat { children: outer } = &children[2] else {
        panic!("{tree:?}");
    };
    assert_eq!(outer.len(), 1);
    let Tree::Group {
        alt: 0,
        children: inner,
    } = &outer[0]
    else {
        panic!("{tree:?}");
    };
    assert!(matches!(&inner[..], [Tree::Repeat { children }] if children.len() == 2));
}

#[test]
fn long_repeats_flatten_into_one_node() {
    let s = setup("start: \"a\"+\n");
    let text: String = core::iter::repeat_n('a', 5000).collect();
    let tree = s.tree(&text);
    let Tree::Rule { children, .. } = &tree else {
        panic!("{tree:?}");
    };
    assert!(matches!(&children[0], Tree::Repeat { children } if children.len() == 5000));
    let leaves = leaves(&tree);
    assert_eq!(leaves.len(), 5000);
    assert!(leaves[0].children().is_empty());
}

#[test]
fn left_and_right_recursion() {
    let left = setup("start: list\nlist: list \"a\" | \"a\"\n");
    let right = setup("start: \"a\" start | \"a\"\n");
    for s in [&left, &right] {
        assert!(!s.accepts(""));
        assert!(s.accepts("a"));
        assert!(s.accepts("aaaa"));
        assert!(!s.accepts("aaab"));
    }
    let text: String = core::iter::repeat_n('a', 3000).collect();
    assert_eq!(leaves(&left.tree(&text)).len(), 3000);
    assert_eq!(leaves(&right.tree(&text)).len(), 3000);
}

#[test]
fn ignored_text_anywhere() {
    let s = setup("start: \"a\" \"b\"\n%ignore \" \"\n%ignore COMMENT\nCOMMENT: /#[^\\n]*\\n/\n");
    assert!(s.accepts("ab"));
    assert!(s.accepts(" ab"));
    assert!(s.accepts("a  b"));
    assert!(s.accepts("ab "));
    assert!(s.accepts("  a # x\n b  "));
    assert!(!s.accepts("a # x b"));
    assert_eq!(s.read_back("  a # x\n b  "), "[ ][ ]a[ ][# x\n][ ]b[ ][ ]");
    let tree = s.tree(" a b ");
    let Tree::Rule { children, .. } = &tree else {
        panic!("{tree:?}");
    };
    let items: Vec<&Tree> = children
        .iter()
        .filter(|c| !matches!(c, Tree::Ignored { .. }))
        .collect();
    assert_eq!(items.len(), 2);
    assert_eq!(children.len(), 5);
    assert!(matches!(
        children[0],
        Tree::Ignored {
            start: 0,
            end: 1,
            ..
        }
    ));
    assert!(matches!(
        children[4],
        Tree::Ignored {
            start: 4,
            end: 5,
            ..
        }
    ));
}

#[test]
fn ignored_text_carries_a_completed_start_rule() {
    let s = setup("start: \"a\"?\n%ignore \" \"\n");
    assert!(s.accepts(""));
    assert!(s.accepts("  "));
    assert!(s.accepts(" a "));
    assert!(!s.accepts(" a a"));
    assert_eq!(s.read_back("  "), "[ ][ ]");
}

#[test]
fn ignored_text_inside_a_recursive_start_rule() {
    let s = setup("start: \"a\" | \"(\" start x \")\"\nx: \"b\" | \n%ignore \" \"\n");
    assert!(s.accepts("( ( a ) b )"));
    assert!(s.accepts("((a) )"));
    assert!(!s.accepts("((a) b"));
}

#[test]
fn terminals_match_greedily_as_lark_does() {
    let s = setup("start: A C\nA: /ab?c?/\nC: /c/\n");
    assert!(s.accepts("acc"));
    assert!(s.accepts("abcc"));
    assert!(!s.accepts("ac"));
    assert!(!s.accepts("abc"));
    assert!(!s.accepts("ab"));
    assert_eq!(s.parse("abc"), Err(ParseError { position: 3 }));
    assert_eq!(s.parse("abcd"), Err(ParseError { position: 3 }));
}

#[test]
fn keywords_and_names_are_tried_separately() {
    let s = setup("start: \"if\" NAME | NAME NAME\nNAME: /[a-z]+/\n%ignore \" \"\n");
    assert!(s.accepts("if x"));
    assert!(s.accepts("iff x"));
    assert!(s.accepts("x if"));
    assert!(!s.accepts("if"));
    assert!(!s.accepts("if x y"));
    let tree = s.tree("if x");
    assert!(matches!(tree, Tree::Rule { alt: 0, .. }));
    let tree = s.tree("ifx y");
    assert!(matches!(tree, Tree::Rule { alt: 1, .. }));
}

#[test]
fn prefixes_and_trailing_text_are_rejected() {
    let s = setup("start: \"(\" (\"a\" | start) \")\"\n");
    assert!(s.accepts("(a)"));
    assert!(s.accepts("(((a)))"));
    assert_eq!(s.parse("(((a))"), Err(ParseError { position: 6 }));
    assert_eq!(s.parse("(((a))))"), Err(ParseError { position: 7 }));
    assert_eq!(s.parse("(((a)b"), Err(ParseError { position: 5 }));
    assert_eq!(s.parse("(("), Err(ParseError { position: 2 }));
}

#[test]
fn declared_terminals_never_match() {
    let s = setup("start: X \"a\" | \"b\"\n%declare X\n");
    assert!(s.accepts("b"));
    assert!(!s.accepts("a"));
    assert!(!s.accepts("xa"));
}

#[test]
fn ambiguous_grammars_parse_once() {
    let s = setup("start: e\ne: e \"+\" e | \"1\"\n");
    assert!(s.accepts("1+1+1+1"));
    let text: String = core::iter::repeat_n("1+", 200)
        .chain(core::iter::once("1"))
        .collect();
    assert_eq!(leaves(&s.tree(&text)).len(), 401);
    assert!(!s.accepts("1+1+"));
}

#[test]
fn an_ambiguous_grammar_parses_a_long_string_within_its_budget() {
    let s = setup("start: e\ne: e \"+\" e | \"1\"\n");
    let text: String = core::iter::repeat_n("1+", 99)
        .chain(core::iter::once("1"))
        .collect();
    assert_eq!(leaves(&s.tree(&text)).len(), 199);
    let strict = Parser::new(&s.grammar, s.grammar.start[0]).with_budget(1000);
    let err = strict.parse(&s.lexer, &chars(&text)).unwrap_err();
    assert!(err.position > 0 && err.position < text.len(), "{err:?}");
    let short: String = core::iter::repeat_n("1+", 4)
        .chain(core::iter::once("1"))
        .collect();
    assert!(strict.accepts(&s.lexer, &chars(&short)));
}

struct FixtureImports;

impl ImportSource for FixtureImports {
    fn load(
        &self,
        base_path: Option<&str>,
        grammar_path: &str,
    ) -> crate::native::lark::GrammarResult<Option<(String, String)>> {
        let Some(base) = base_path else {
            return Ok(None);
        };
        let path = Path::new(base).join(grammar_path);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };
        Ok(Some((path.to_str().unwrap().to_string(), text)))
    }
}

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lark")
}

/// The fixture grammars whose strings Lark can parse without a
/// postlexer (those that `%declare` terminals need one) and in
/// reasonable time (one it cannot says so with a `// corpus: no` line),
/// each with its first start rule.
fn corpus_grammars() -> Vec<(String, String)> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "lark"))
        .map(|path| path.file_stem().unwrap().to_str().unwrap().to_string())
        .filter(|name| !name.starts_with("err_"))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter(|name| {
            let text = std::fs::read_to_string(fixtures().join(format!("{name}.lark"))).unwrap();
            !text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("%declare") || line == "// corpus: no"
            })
        })
        .map(|name| {
            let expected =
                std::fs::read_to_string(fixtures().join(format!("{name}.expected"))).unwrap();
            let start = expected
                .lines()
                .next()
                .and_then(|line| line.strip_prefix("start "))
                .and_then(|starts| starts.split_whitespace().next())
                .unwrap()
                .to_string();
            (name, start)
        })
        .collect()
}

fn fixture(name: &str, start: &str) -> Grammar {
    let path = fixtures().join(format!("{name}.lark"));
    let text = std::fs::read_to_string(&path).unwrap();
    let built = load_grammar(&text, Some(path.to_str().unwrap()), &FixtureImports).unwrap();
    compile(&built, &[start]).unwrap()
}

fn generated(
    grammar: &Grammar,
    start: &str,
    max_size: u64,
    seeds: core::ops::Range<u64>,
) -> Vec<String> {
    let spec = GrammarSpec::new(
        grammar.clone(),
        GrammarOptions {
            start: start.to_string(),
            max_size,
            ..GrammarOptions::default()
        },
    )
    .unwrap();
    seeds
        .filter_map(|seed| {
            let mut ntc = NativeTestCase::new_random(EngineRng::seeded(seed)).unwrap();
            generate_grammar(&mut ntc, &spec, None).ok()
        })
        .collect()
}

fn check_fixtures(max_size: u64, seeds: core::ops::Range<u64>) {
    for (name, start) in corpus_grammars() {
        let grammar = fixture(&name, &start);
        let s = build(grammar);
        let strings = generated(&s.grammar, &start, max_size, seeds.clone());
        assert!(!strings.is_empty(), "{name}");
        let started = std::time::Instant::now();
        let mut chars_parsed = 0;
        let mut slowest = (core::time::Duration::ZERO, 0);
        for string in &strings {
            let text = chars(string);
            chars_parsed += text.len();
            let one = std::time::Instant::now();
            let tree = s
                .parser
                .parse(&s.lexer, &text)
                .unwrap_or_else(|e| panic!("{name}: {e:?} parsing {string:?}"));
            slowest = slowest.max((one.elapsed(), text.len()));
            assert!(covers(&tree, &text), "{name}: {string:?}");
            let expected: String = text.iter().collect();
            assert_eq!(
                read_back(&tree, &text).replace(['[', ']'], ""),
                expected.replace(['[', ']'], "")
            );
        }
        std::eprintln!(
            "{name}: {} strings, {chars_parsed} chars, {:?}; slowest {:?} for {} chars",
            strings.len(),
            started.elapsed(),
            slowest.0,
            slowest.1
        );
    }
}

#[test]
fn fixture_grammars_parse_their_own_strings() {
    check_fixtures(2000, 0..4);
}

/// The ambiguous fixture is kept out of the corpus, Lark's own parse
/// of its strings being cubic too; the engine parses a string of over a
/// thousand characters from it within its budget, in well under a
/// second.
#[test]
fn the_ambiguous_fixture_parses_its_own_strings() {
    assert!(
        !corpus_grammars()
            .iter()
            .any(|(name, _)| name == "ambiguous")
    );
    let grammar = fixture("ambiguous", "start");
    let s = build(grammar);
    let strings = generated(&s.grammar, "start", 3000, 0..30);
    let longest = strings
        .iter()
        .filter(|s| s.len() <= 1600)
        .max_by_key(|s| s.len())
        .unwrap();
    assert!(longest.len() > 1000, "{}", longest.len());
    let text = chars(longest);
    let tree = s.parser.parse(&s.lexer, &text).unwrap();
    assert!(covers(&tree, &text));
    assert_eq!(leaves(&tree).len(), text.len());
}

#[test]
#[ignore]
fn fixture_grammars_parse_their_own_strings_at_full_size() {
    check_fixtures(DEFAULT_MAX_SIZE, 0..40);
}

/// Writes the parser's verdict on candidate strings for the oracle check
/// (`scripts/check-parse.py`): for each grammar named in
/// `HEGEL_LARK_CORPUS_GRAMMARS`, reads the JSON strings in
/// `<HEGEL_LARK_VERDICT_DIR>/<name>.candidates.jsonl` and writes `true`
/// or `false` per line to `<name>.verdicts.jsonl` beside it.
#[test]
#[ignore]
fn write_verdicts() {
    let dir = std::env::var("HEGEL_LARK_VERDICT_DIR").unwrap();
    let grammars = std::env::var("HEGEL_LARK_CORPUS_GRAMMARS").unwrap();
    let starts = corpus_grammars();
    for name in grammars.split(',').filter(|name| !name.is_empty()) {
        let (_, start) = starts.iter().find(|(n, _)| n == name).unwrap();
        let s = build(fixture(name, start));
        let input =
            std::fs::read_to_string(Path::new(&dir).join(format!("{name}.candidates.jsonl")))
                .unwrap();
        let mut lines = String::new();
        let started = std::time::Instant::now();
        let mut count = 0;
        for line in input.lines().filter(|line| !line.is_empty()) {
            let candidate: String = serde_json::from_str(line).unwrap();
            let text = chars(&candidate);
            let verdict = match s.parser.parse(&s.lexer, &text) {
                Ok(tree) => {
                    assert!(covers(&tree, &text), "{name}: {candidate:?}");
                    true
                }
                Err(_) => false,
            };
            lines.push_str(if verdict { "true\n" } else { "false\n" });
            count += 1;
        }
        std::fs::write(
            Path::new(&dir).join(format!("{name}.verdicts.jsonl")),
            lines,
        )
        .unwrap();
        std::eprintln!("{name}: {count} candidates, {:?}", started.elapsed());
    }
}

#[test]
fn right_recursion_takes_a_column_of_items_per_character() {
    let right = setup("start: \"a\" start | \"a\"\n");
    let list = setup("start: list\nlist: item \",\" list | item\nitem: \"a\"\n");
    let text: String = core::iter::repeat_n('a', 8192).collect();
    let items: String = core::iter::repeat_n("a,", 4095)
        .chain(core::iter::once("a"))
        .collect();
    for (s, text) in [(&right, &text), (&list, &items)] {
        let lean = Parser::new(&s.grammar, s.grammar.start[0]).with_item_budget(100_000);
        let tree = lean.parse(&s.lexer, &chars(text)).unwrap();
        assert!(covers(&tree, &chars(text)));
        assert_eq!(
            read_back(&tree, &chars(text)).replace(['[', ']'], ""),
            *text
        );
    }
    for g in [
        "start: x\nx: x | \"a\" x | \"a\"\n",
        "start: start | \"a\" start | \"a\"\n",
    ] {
        let cycle = setup(g);
        assert!(cycle.accepts("aaa"));
        assert_eq!(leaves(&cycle.tree("aaa")).len(), 3);
        assert!(!cycle.accepts("aab"));
    }
}

#[test]
fn a_parse_holding_too_many_items_is_given_up() {
    let s = setup("start: e\ne: e \"+\" e | \"1\"\n");
    let text: String = core::iter::repeat_n("1+", 20)
        .chain(core::iter::once("1"))
        .collect();
    let strict = Parser::new(&s.grammar, s.grammar.start[0]).with_item_budget(200);
    let err = strict.parse(&s.lexer, &chars(&text)).unwrap_err();
    assert!(err.position > 0 && err.position < text.len(), "{err:?}");
    assert!(strict.accepts(&s.lexer, &chars("1+1")));
}

#[test]
fn a_derivation_as_deep_as_its_text_is_long_needs_no_call_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let s = setup("start: \"(\" start \")\" | \"a\"\n");
            let depth = 20_000;
            let text: String = core::iter::repeat_n('(', depth)
                .chain(core::iter::once('a'))
                .chain(core::iter::repeat_n(')', depth))
                .collect();
            let tree = s.tree(&text);
            assert_eq!(tree.size(), 3 * depth + 2);
            let layout = Layout::new(&tree);
            assert_eq!(layout.depth() as usize, depth + 1);
            let pairs: u32 = (1..=depth as u32)
                .map(|i| u32::BITS - i.leading_zeros())
                .sum();
            assert_eq!(layout.hoists().len(), pairs as usize);
            drop(layout);
            drop(tree);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn the_right_recursion_shortcut_keeps_a_completed_start_rule() {
    let s = setup("start: t \"c\" | t \"d\" | x\nx: \"a\"\nt: start\n");
    assert!(s.accepts("a"));
    assert!(s.accepts("ac"));
    assert!(s.accepts("acd"));
    assert!(s.accepts("acc"));
    assert!(!s.accepts("ca"));
    assert_eq!(s.read_back("acd"), "acd");
    let ignoring = setup("start: t \"c\" | t \"d\" | x\nx: \"a\"\nt: start\n%ignore \" \"\n");
    assert!(ignoring.accepts("a "));
    assert!(ignoring.accepts(" a c"));
    assert_eq!(ignoring.read_back("a "), "a[ ]");
}

#[test]
fn unit_cycles_have_two_waiters_wherever_they_are_entered() {
    for (g, text) in [
        ("start: a\na: b | \"y\"\nb: a\n", "y"),
        ("start: \"x\" a\na: b | \"y\"\nb: a\n", "xy"),
        (
            "start: \"x\" a \"z\"\na: b | \"y\"\nb: a | n a\nn:\n",
            "xyz",
        ),
        ("start: a | \"x\" start\na: start | \"y\"\n", "xxy"),
    ] {
        let s = setup(g);
        assert!(s.accepts(text), "{g}");
        assert_eq!(s.read_back(text), text, "{g}");
        assert!(!s.accepts("yy"), "{g}");
    }
}
