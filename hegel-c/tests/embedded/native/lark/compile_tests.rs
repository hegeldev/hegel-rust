//! Direct-equivalent tests for `src/native/lark/compile.rs`, including a
//! comparison against what Lark itself compiles each grammar in
//! `hegel-c/tests/fixtures/lark/` to (see `scripts/gen-lark-fixtures.py`).

extern crate std;

use super::*;
use crate::native::lark::builder::{ImportSource, NoImports, load_grammar};
use alloc::string::ToString;
use alloc::vec;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lark")
}

/// Loads `%import`ed grammars from the fixtures directory, the way Lark
/// resolves imports relative to the importing file.
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

/// Decode the JSON string literal the fixture generator writes regexps as.
fn decode_json_string(s: &str) -> String {
    let inner = s.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next().unwrap() {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\x08'),
            'f' => out.push('\x0c'),
            'u' => {
                let hex: String = chars.by_ref().take(4).collect();
                let mut code = u32::from_str_radix(&hex, 16).unwrap();
                if (0xD800..0xDC00).contains(&code) {
                    assert_eq!(chars.next(), Some('\\'));
                    assert_eq!(chars.next(), Some('u'));
                    let low: String = chars.by_ref().take(4).collect();
                    let low = u32::from_str_radix(&low, 16).unwrap();
                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                }
                out.push(char::from_u32(code).unwrap());
            }
            other => out.push(other),
        }
    }
    out
}

fn start_rules(text: &str) -> Vec<&str> {
    text.lines()
        .find_map(|line| line.strip_prefix("// start: "))
        .map(|s| s.split_whitespace().collect())
        .unwrap_or_else(|| vec!["start"])
}

fn compile_fixture(path: &Path) -> GrammarResult<Grammar> {
    let text = std::fs::read_to_string(path).unwrap();
    let starts = start_rules(&text);
    let built = load_grammar(&text, Some(path.to_str().unwrap()), &FixtureImports)?;
    compile(&built, &starts)
}

/// Lark numbers anonymous terminals in the order its tree transformer
/// visits literals (deepest first); the port numbers them left to right.
/// The numbering carries no meaning, so compare them by pattern instead.
fn anon_normalized(name: &str) -> &str {
    if name.starts_with("__ANON_") {
        "__ANON"
    } else {
        name
    }
}

fn describe(grammar: &Grammar, starts: &[&str]) -> String {
    let mut lines = vec![format!("start {}", starts.join(" "))];
    let mut terminals: Vec<(&str, i64, &str)> = grammar
        .terminals
        .iter()
        .filter(|t| t.pattern.is_some())
        .map(|t| {
            (
                anon_normalized(&t.name),
                t.priority,
                t.regexp.as_deref().unwrap(),
            )
        })
        .collect();
    terminals.sort_unstable();
    for (name, priority, regexp) in terminals {
        lines.push(format!("terminal {name}\t{priority}\t{regexp}"));
    }
    for &id in &grammar.ignore {
        lines.push(format!("ignore {}", grammar.terminals[id].name));
    }
    let mut rules: Vec<&str> = grammar.rules.iter().map(|r| r.name.as_str()).collect();
    rules.sort_unstable();
    for name in rules {
        lines.push(format!("rule {name}"));
    }
    lines.join("\n") + "\n"
}

fn decode_expected(expected: &str) -> String {
    let mut out = String::new();
    let mut terminals = Vec::new();
    let mut rest = Vec::new();
    for line in expected.lines() {
        if let Some(line) = line.strip_prefix("terminal ") {
            let mut parts = line.splitn(3, '\t');
            let name = anon_normalized(parts.next().unwrap());
            let priority: i64 = parts.next().unwrap().parse().unwrap();
            let regexp = decode_json_string(parts.next().unwrap());
            terminals.push((name, priority, regexp));
        } else if line.starts_with("start ") {
            out.push_str(line);
            out.push('\n');
        } else {
            rest.push(line);
        }
    }
    terminals.sort_unstable();
    for (name, priority, regexp) in terminals {
        out.push_str(&format!("terminal {name}\t{priority}\t{regexp}\n"));
    }
    for line in rest {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[test]
fn fixtures_match_lark() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "lark"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 40);
    let mut failures = Vec::new();
    for path in paths {
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let expected = std::fs::read_to_string(path.with_extension("expected")).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let starts = start_rules(&text);
        let result = compile_fixture(&path);
        if name.starts_with("err_") {
            if result.is_ok() {
                failures.push(format!(
                    "{name}: compiled, but Lark rejects it with: {}",
                    expected.lines().nth(1).unwrap_or("")
                ));
            }
            continue;
        }
        match result {
            Err(e) => failures.push(format!("{name}: {}", e.message)),
            Ok(grammar) => {
                let actual = describe(&grammar, &starts);
                let expected = decode_expected(&expected);
                if actual != expected {
                    let diff: Vec<String> = actual
                        .lines()
                        .zip(expected.lines())
                        .filter(|(a, e)| a != e)
                        .map(|(a, e)| format!("    got:      {a}\n    expected: {e}"))
                        .collect();
                    failures.push(format!(
                        "{name}: differs from Lark ({} vs {} lines)\n{}",
                        actual.lines().count(),
                        expected.lines().count(),
                        diff.join("\n")
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn compiled(text: &str) -> Grammar {
    compile(&load_grammar(text, None, &NoImports).unwrap(), &["start"]).unwrap()
}

fn compile_err(text: &str) -> String {
    let built = match load_grammar(text, None, &NoImports) {
        Ok(built) => built,
        Err(e) => return e.message,
    };
    compile(&built, &["start"]).unwrap_err().message
}

fn rule<'g>(grammar: &'g Grammar, name: &str) -> &'g Rule {
    &grammar.rules[grammar.rule_id(name).unwrap()]
}

#[test]
fn rules_keep_their_structure() {
    let grammar = compiled("start: a? (B | \"c\")+ a~2..3 [a]\na: \"x\"\nB: \"b\"");
    let a = grammar.rule_id("a").unwrap();
    let b = grammar.terminal_id("B").unwrap();
    let c = grammar.terminal_id("C").unwrap();
    assert_eq!(
        rule(&grammar, "start").alternatives,
        vec![Sequence {
            items: vec![
                Item::Repeat {
                    inner: Box::new(Item::Rule(a)),
                    min: 0,
                    max: Some(1)
                },
                Item::Repeat {
                    inner: Box::new(Item::Group(vec![
                        Sequence {
                            items: vec![Item::Terminal(b)]
                        },
                        Sequence {
                            items: vec![Item::Terminal(c)]
                        },
                    ])),
                    min: 1,
                    max: None
                },
                Item::Repeat {
                    inner: Box::new(Item::Rule(a)),
                    min: 2,
                    max: Some(3)
                },
                Item::Repeat {
                    inner: Box::new(Item::Group(vec![Sequence {
                        items: vec![Item::Rule(a)]
                    }])),
                    min: 0,
                    max: Some(1)
                },
            ]
        }]
    );
    assert_eq!(grammar.start, vec![grammar.rule_id("start").unwrap()]);
}

#[test]
fn terminal_widths_are_recorded() {
    let grammar = compiled("start: A B\nA: \"ab\" | \"abc\"\nB: /x+/");
    let a = &grammar.terminals[grammar.terminal_id("A").unwrap()];
    assert_eq!((a.min_width, a.max_width), (2, Some(3)));
    let b = &grammar.terminals[grammar.terminal_id("B").unwrap()];
    assert_eq!((b.min_width, b.max_width), (1, None));
}

#[test]
fn a_literal_shared_by_named_terminals_resolves_to_the_last_defined() {
    let grammar = compiled("start: \"x\" \"y\"\nA: \"x\"\nB: \"x\"\nC: \"y\"");
    let start = &grammar.rules[grammar.rule_id("start").unwrap()];
    assert_eq!(
        start.alternatives[0].items,
        vec![
            Item::Terminal(grammar.terminal_id("B").unwrap()),
            Item::Terminal(grammar.terminal_id("C").unwrap()),
        ]
    );
}

#[test]
fn declared_terminals_have_no_pattern() {
    let grammar = compiled("start: A \"x\"\n%declare A");
    let a = &grammar.terminals[grammar.terminal_id("A").unwrap()];
    assert_eq!(a.pattern, None);
    assert_eq!(a.regexp, None);
    assert_eq!(a.min_width, 0);
}

#[test]
fn templates_instantiate_once_per_argument_list() {
    let grammar = compiled(
        "start: sep{A, \",\"} sep{A, \",\"} sep{B, \";\"} nested{A}\nsep{x, d}: x (d x)*\nnested{y}: sep{y, \"|\"}\nA: \"a\"\nB: \"b\"",
    );
    let mut names: Vec<&str> = grammar.rules.iter().map(|r| r.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "nested{A}",
            "sep{A,COMMA}",
            "sep{A,VBAR}",
            "sep{B,SEMICOLON}",
            "start"
        ]
    );
    let sep = rule(&grammar, "sep{A,COMMA}");
    let a = grammar.terminal_id("A").unwrap();
    let comma = grammar.terminal_id("COMMA").unwrap();
    assert_eq!(
        sep.alternatives,
        vec![Sequence {
            items: vec![
                Item::Terminal(a),
                Item::Repeat {
                    inner: Box::new(Item::Group(vec![Sequence {
                        items: vec![Item::Terminal(comma), Item::Terminal(a)]
                    }])),
                    min: 0,
                    max: None
                }
            ]
        }]
    );
}

#[test]
fn template_used_without_arguments_is_an_error() {
    assert!(compile_err("start: t\nt{x}: x").contains("Template 't' is used without arguments"));
}

#[test]
fn deep_aliases_are_rejected() {
    assert!(compile_err("start: (\"a\" -> b) \"c\"\nb: \"b\"").contains("Deep aliasing"));
}

#[test]
fn inlined_rules_cannot_have_aliases() {
    assert!(
        compile_err("start: _a\n_a: \"x\" -> y")
            .contains("Rule _a is marked for expansion (it starts with an underscore) and isn't allowed to have aliases (alias=y)")
    );
}

#[test]
fn terminal_errors() {
    assert!(compile_err("start: A\nA: \"a\" -> b").contains("Aliasing not allowed in terminals"));
    assert!(
        compile_err("start: A\nA: t{\"a\"}\nt{x}: x")
            .contains("Templates cannot be used inside terminals (A)")
    );
    assert!(compile_err("start: A\nA:").contains("Terminals cannot be empty (A)"));
    assert!(
        compile_err("start: A\nA: \"a\"?")
            .contains("Zero-width terminals are not allowed (A: (?:a)?)")
    );
    assert!(compile_err("start: A\nA: /(/").contains("Cannot compile token A"));
    assert!(
        compile_err("start: A\nA: \"a\"~0..0").contains("Zero-width terminals are not allowed")
    );
}

#[test]
fn missing_start_rule() {
    assert!(compile_err("other: \"a\"").contains("Start rule 'start' is not defined"));
}

#[test]
fn unreachable_rules_and_terminals_are_dropped_but_ignored_terminals_kept() {
    let grammar =
        compiled("start: a\na: \"a\"\nb: \"b\" UNUSED\nUNUSED: \"u\"\nKEPT: \"k\"\n%ignore KEPT");
    assert_eq!(grammar.rule_id("b"), None);
    assert_eq!(grammar.terminal_id("UNUSED"), None);
    assert_eq!(grammar.ignore, vec![grammar.terminal_id("KEPT").unwrap()]);
}

#[test]
fn anonymous_terminal_naming() {
    let grammar =
        compiled("start: \"if\" \"+\" /x/ /x/ \"1a\" \"IF\" \"é\" \"split\"\nSPLIT: \"s\"");
    let names: Vec<&str> = grammar.terminals.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "IF", "PLUS", "__ANON_0", "__ANON_1", "__ANON_2", "É", "__ANON_3"
        ]
    );
}

#[test]
fn grammar_lookups() {
    let grammar = compiled("start: A\nA: \"a\"");
    assert_eq!(grammar.rule_id("missing"), None);
    assert_eq!(grammar.terminal_id("MISSING"), None);
}

#[test]
fn a_template_parameter_used_as_a_template_is_rejected_like_lark() {
    let compiled = |text: &str| compile(&load_grammar(text, None, &NoImports).unwrap(), &["start"]);
    let err = compiled("start: tmpl{inner}\ntmpl{x}: x{\"a\"}\ninner{y}: y \"b\"\n").unwrap_err();
    assert_eq!(err.message, "Template 'inner' is used without arguments");
    let err = compiled("start: tmpl{A}\ntmpl{x}: x{\"a\"}\nA: \"q\"\n").unwrap_err();
    assert_eq!(
        err.message,
        "Template 'A' used but not defined (in rule tmpl{A})"
    );
    let err = compiled("start: tmpl{r}\ntmpl{x}: x{\"a\"}\nr: \"q\"\n").unwrap_err();
    assert_eq!(
        err.message,
        "Template 'r' used but not defined (in rule tmpl{r})"
    );
}

#[test]
fn a_template_instantiating_itself_ever_deeper_is_rejected() {
    let err = compile_err("start: t{\"a\"}\nt{x}: x | t{t{x}}\n");
    assert!(
        err.starts_with(
            "Template 't' nests its instantiations more than 200 levels deep (in rule t{t{t{"
        ) && err.ends_with("})")
            && err.matches('{').count() == 200,
        "{err}"
    );
    let err = compile_err("start: t{\"a\"}\nt{x}: x | u{t{x}}\nu{y}: y | t{u{y}}\n");
    assert!(
        err.contains("nests its instantiations more than 200 levels deep"),
        "{err}"
    );
    compiled("start: t{t{t{\"a\"}}}\nt{x}: x | \"(\" x \")\"\n");
}

#[test]
fn only_the_terminals_kept_are_compiled_and_checked_for_width() {
    let grammar = compiled("start: A\n_INNER: /.*?/\nA: \"x\" _INNER \"y\"");
    assert_eq!(grammar.terminals.len(), 1);
    assert_eq!(grammar.terminals[0].regexp.as_deref(), Some("x.*?y"));
    compiled("start: \"a\"\nUNUSED: /a*/");
    compiled("start: \"a\"\nUNUSED: /(/");
    compiled("start: \"a\"\nx: BAD\nBAD: /(/");
    assert!(
        compile_err("start: \"a\"\n%ignore BAD\nBAD: /(/").contains("Cannot compile token BAD")
    );
    assert!(
        compile_err("start: \"a\"\n%ignore EMPTY\nEMPTY: /a*/")
            .contains("Zero-width terminals are not allowed (EMPTY: a*)")
    );
}

#[test]
fn the_number_of_template_instantiations_is_capped() {
    let chain = |instantiations: usize| {
        let mut text = String::from("t{x}: x\nstart:");
        for i in 0..instantiations {
            text.push_str(&format!(" t{{a{i}}}"));
        }
        text.push('\n');
        for i in 0..instantiations {
            text.push_str(&format!("a{i}: \"a\"\n"));
        }
        text
    };
    let grammar = compiled(&chain(4096));
    assert_eq!(grammar.rules.len(), 1 + 2 * 4096);
    assert_eq!(
        compile_err(&chain(4097)),
        "Templates are instantiated more than 4096 times (instantiating 't' in rule start)"
    );
}
