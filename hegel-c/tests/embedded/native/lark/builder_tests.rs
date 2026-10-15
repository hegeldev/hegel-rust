//! Direct-equivalent tests for `src/native/lark/builder.rs`.

use super::*;
use crate::native::lark::ast::{Alternative, Pattern};
use crate::native::lark::compile::compile;
use alloc::vec;

/// An in-memory import source: grammar texts by path.
struct Sources(Vec<(&'static str, &'static str)>);

impl ImportSource for Sources {
    fn read(&self, path: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, text)| text.to_string())
    }
}

fn build(text: &str) -> BuiltGrammar {
    load_grammar(text, &NoImports).unwrap()
}

fn build_with(text: &str, sources: &Sources) -> GrammarResult<BuiltGrammar> {
    load_grammar(text, sources)
}

/// Every path a [`GrammarFiles`] lists, in order.
fn paths(files: &GrammarFiles) -> Vec<&str> {
    (0..).map_while(|i| files.path(i)).collect()
}

#[test]
fn circular_imports_report_the_cycle() {
    let sources = Sources(vec![
        (
            "a.lark",
            "%import b.value
",
        ),
        (
            "b.lark",
            "%import a.value
",
        ),
    ]);
    let error = build_with(
        "%import a.value
start: value",
        &sources,
    )
    .unwrap_err();
    assert_eq!(
        error.message,
        "Circular grammar import: a.lark -> b.lark -> a.lark"
    );
}

#[test]
fn import_depth_is_bounded_while_gathering_files() {
    struct Chain;

    impl ImportSource for Chain {
        fn read(&self, path: &str) -> Option<String> {
            let i: usize = path[1..path.len() - 5].parse().unwrap();
            assert!(i < 100);
            Some(format!("%import n{}.value\n", i + 1))
        }
    }

    let error = load_grammar("%import n0.value\nstart: value", &Chain).unwrap_err();
    assert!(
        error
            .message
            .contains("Grammar imports nest more than 64 levels deep"),
        "{}",
        error.message
    );
}

#[test]
fn import_depth_is_bounded_while_building() {
    let root: String = (0..71)
        .map(|i| format!("%import n{i}.value -> v{i}\n"))
        .collect();
    let mut files = GrammarFiles::new(format!("{root}start: v0\n")).unwrap();
    assert_eq!(paths(&files).len(), 71);
    for i in 0..70 {
        let text = format!("%import n{}.value\nvalue: \"a\"\n", i + 1);
        files.provide(i, Some(text)).unwrap();
    }
    files
        .provide(70, Some(String::from("value: \"a\"\n")))
        .unwrap();
    let error = files.build().unwrap_err();
    assert_eq!(
        error.message,
        "Grammar imports nest more than 64 levels deep"
    );
}

#[test]
fn import_paths_join_the_dotted_path_and_relative_imports_the_importers_directory() {
    let mut files = GrammarFiles::new(String::from(
        "%import a.b.x\n%import .c.y\n%import a.b (z)\nstart: x y z\n",
    ))
    .unwrap();
    assert_eq!(paths(&files), vec!["a/b.lark", "c.lark"]);
    files
        .provide(
            0,
            Some(String::from("%import .d.x\n%import e.y\n%import .d (w)\n")),
        )
        .unwrap();
    assert_eq!(
        paths(&files),
        vec!["a/b.lark", "c.lark", "a/d.lark", "e.lark"]
    );
    files
        .provide(1, Some(String::from("%import .f.x\n")))
        .unwrap();
    assert_eq!(paths(&files)[4], "f.lark");
    files
        .provide(2, Some(String::from("%import .g.x\n")))
        .unwrap();
    assert_eq!(paths(&files)[5], "a/g.lark");
}

#[test]
fn each_path_is_listed_once() {
    let mut files =
        GrammarFiles::new(String::from("%import a.x\n%import b.y\nstart: x y\n")).unwrap();
    files
        .provide(0, Some(String::from("%import b.z\n")))
        .unwrap();
    assert_eq!(paths(&files), vec!["a.lark", "b.lark"]);
}

#[test]
fn a_missing_file_falls_back_to_the_bundled_grammar_with_its_path() {
    let mut files = GrammarFiles::new(String::from("%import lark.RULE\nstart: RULE\n")).unwrap();
    files.provide(0, None).unwrap();
    assert_eq!(paths(&files), vec!["lark.lark", "common.lark"]);
    files.provide(1, None).unwrap();
    assert_eq!(term_names(&files.build().unwrap()), vec!["RULE"]);
}

#[test]
fn the_frontends_file_takes_precedence_over_the_bundled_one() {
    let sources = Sources(vec![("common.lark", "INT: \"7\"\n")]);
    let built = build_with("%import common.INT\nstart: INT", &sources).unwrap();
    assert_eq!(
        term_body(&built, "INT"),
        &Expr::Alternatives(vec![alt(vec![Expr::Literal(Pattern::str("7"))])])
    );
}

#[test]
fn a_bundled_grammar_stands_in_only_at_its_own_path() {
    let mut files = GrammarFiles::new(String::from("%import a.b.x\nstart: x\n")).unwrap();
    files
        .provide(0, Some(String::from("%import .common.INT\nx: INT\n")))
        .unwrap();
    assert_eq!(
        files.provide(1, None).unwrap_err().message,
        "Could not find grammar `a/common.lark` to import"
    );
}

#[test]
fn providing_reports_bad_indices_and_bad_text() {
    let mut files = GrammarFiles::new(String::from("%import a.x\nstart: x\n")).unwrap();
    assert_eq!(
        files.provide(1, None).unwrap_err().message,
        "There is no grammar file 1: only 1 are needed so far"
    );
    assert!(
        files
            .provide(0, Some(String::from("x: (")))
            .unwrap_err()
            .message
            .starts_with("In grammar `a.lark`: ")
    );
    assert_eq!(
        files.build().unwrap_err().message,
        "Grammar file `a.lark` was never provided"
    );
    files.provide(0, Some(String::from("x: \"x\"\n"))).unwrap();
    assert_eq!(
        files.provide(0, None).unwrap_err().message,
        "Grammar file `a.lark` was already provided"
    );
    assert_eq!(rule_names(&files.build().unwrap()), vec!["x", "start"]);
}

#[test]
fn a_root_text_that_does_not_parse_is_an_error() {
    assert!(GrammarFiles::new(String::from("start: (")).is_err());
}

#[test]
fn independent_imports_can_share_a_dependency() {
    let sources = Sources(vec![
        ("a.lark", "%import shared.VALUE -> A\n"),
        ("b.lark", "%import shared.VALUE -> B\n"),
        ("shared.lark", "VALUE: \"x\"\n"),
    ]);
    let built = build_with("%import a.A\n%import b.B\nstart: A B", &sources).unwrap();
    assert_eq!(term_names(&built), vec!["A", "B"]);
}

fn err(text: &str) -> String {
    load_grammar(text, &NoImports).unwrap_err().message
}

fn rule_names(built: &BuiltGrammar) -> Vec<&str> {
    built.rule_defs.iter().map(|r| r.0.as_str()).collect()
}

fn term_names(built: &BuiltGrammar) -> Vec<&str> {
    built.term_defs.iter().map(|t| t.0.as_str()).collect()
}

fn term_body<'b>(built: &'b BuiltGrammar, name: &str) -> &'b Expr {
    built
        .term_defs
        .iter()
        .find(|t| t.0 == name)
        .unwrap()
        .1
        .as_ref()
        .unwrap()
}

fn alt(items: Vec<Expr>) -> Alternative {
    Alternative { items, alias: None }
}

#[test]
fn definitions_in_order_with_options() {
    let built = build("start: a\n?a.2: \"x\"\nA.3: \"y\"\n%declare B");
    assert_eq!(rule_names(&built), vec!["start", "a"]);
    assert_eq!(term_names(&built), vec!["A", "B"]);
    let a = &built.rule_defs[1];
    assert_eq!(a.3.priority, Some(2));
    assert!(a.3.modifiers.expand1);
    assert_eq!(built.term_defs[0].2, 3);
    assert_eq!(built.term_defs[1].1, None);
}

#[test]
fn duplicate_definitions() {
    assert!(err("a: \"x\"\na: \"y\"").contains("Rule 'a' defined more than once"));
    assert!(err("A: \"x\"\nA: \"y\"").contains("Terminal 'A' defined more than once"));
    assert!(err("A: \"x\"\n%declare A").contains("Terminal 'A' defined more than once"));
}

#[test]
fn override_and_extend() {
    let built = build(
        "start: a\na: \"x\"\n%override a: \"y\"\n%extend a: \"z\"\nA: \"1\"\n%extend A: \"2\"",
    );
    let a = &built.rule_defs[1].2;
    assert_eq!(
        a,
        &Expr::Alternatives(vec![
            alt(vec![Expr::Literal(Pattern::str("z"))]),
            alt(vec![Expr::Literal(Pattern::str("y"))]),
        ])
    );
    assert_eq!(
        term_body(&built, "A"),
        &Expr::Alternatives(vec![
            alt(vec![Expr::Literal(Pattern::str("2"))]),
            alt(vec![Expr::Literal(Pattern::str("1"))]),
        ])
    );
}

#[test]
fn extend_puts_the_new_alternatives_first_with_their_aliases() {
    let built = build("start: a\na: \"x\" -> x\n%extend a: \"y\" -> y | \"z\"");
    assert_eq!(
        &built.rule_defs[1].2,
        &Expr::Alternatives(vec![
            Alternative {
                items: vec![Expr::Literal(Pattern::str("y"))],
                alias: Some(String::from("y")),
            },
            alt(vec![Expr::Literal(Pattern::str("z"))]),
            Alternative {
                items: vec![Expr::Literal(Pattern::str("x"))],
                alias: Some(String::from("x")),
            },
        ])
    );
    compile(&built, &["start"]).unwrap();
}

#[test]
fn override_and_extend_errors() {
    assert!(err("%override a: \"x\"").contains("Cannot override a nonexisting rule a"));
    assert!(err("%override A: \"x\"").contains("Cannot override a nonexisting terminal A"));
    assert!(err("%extend a: \"x\"").contains("Can't extend rule a as it wasn't defined before"));
    assert!(
        err("a: \"x\"\n%extend A: \"y\"")
            .contains("Can't extend terminal A as it wasn't defined before")
    );
    assert!(err("A: \"x\"\n%extend A: \"y\"\n%extend a: \"z\"").contains("Can't extend rule a"));
    assert!(
        err("t{x}: x\n%extend t{y}: y").contains("Cannot extend rule with different parameters: t")
    );
    assert!(
        err("%declare A\n%extend A: \"y\"").contains("Can't extend terminal A - it is abstract.")
    );
    assert_eq!(
        err("%import common.INT -> int\n%extend int: \"x\"\nstart: \"a\""),
        "Cannot extend rule int - one is a terminal, while the other is not."
    );
    assert_eq!(
        err("%import python.starguard -> STARGUARD\n%extend STARGUARD: \"x\"\nstart: \"a\""),
        "Cannot extend terminal STARGUARD - one is a terminal, while the other is not."
    );
}

#[test]
fn extend_applies_to_imported_rules() {
    let sources = Sources(vec![("other.lark", "a: \"x\"\n")]);
    let result = build_with("%import other.a\n%extend a: \"y\"\nstart: a", &sources);
    assert!(result.is_ok());
}

#[test]
fn declare_rejects_rule_names() {
    assert!(
        err("start: \"a\"\n%declare a")
            .contains("Expecting terminal name to follow %declare, but got rule name 'a'")
    );
}

#[test]
fn double_underscore_names_cannot_be_written() {
    assert!(err("%import common.INT -> __x\nstart: __x").contains("Expecting a name"));
    assert!(err("__x: \"a\"").contains("Expecting a name"));
}

#[test]
fn ignore_by_name_and_by_expression() {
    let built =
        build("start: \"a\"\nWS: \" \"\n%ignore WS\n%ignore \"\\t\" | /#.*/\n%ignore \"x\"");
    assert_eq!(built.ignore, vec!["WS", "__IGNORE_1", "__IGNORE_2"]);
    assert_eq!(term_names(&built), vec!["WS", "__IGNORE_1", "__IGNORE_2"]);
    assert!(err("start: \"a\"\n%ignore \"x\" -> y").contains("Aliasing not allowed in terminals"));
}

#[test]
fn undefined_ignore() {
    assert!(
        err("start: \"a\"\n%ignore WS")
            .contains("Terminals {'WS'} were marked to ignore but were not defined!")
    );
}

#[test]
fn undefined_symbols() {
    assert!(err("start: b").contains("Rule 'b' used but not defined (in rule start)"));
    assert!(err("start: B").contains("Terminal 'B' used but not defined (in rule start)"));
    assert!(err("start: t{\"a\"}").contains("Template 't' used but not defined (in rule start)"));
    assert!(err("start: t{\"a\", \"b\"}\nt{x}: x").contains(
        "Wrong number of template arguments used for t (expected 1, got 2) (in rule start)"
    ));
}

#[test]
fn template_parameter_errors() {
    assert!(
        err("start: \"a\"\nt{start}: start")
            .contains("Template Parameter conflicts with rule start (in template t)")
    );
    assert!(err("t{x, x}: x").contains("Duplicate Template Parameter x (in template t)"));
}

#[test]
fn template_parameters_are_not_undefined_symbols() {
    let built = build("start: t{\"a\"}\nt{x}: x u{x}\nu{y}: y");
    assert_eq!(rule_names(&built), vec!["start", "t", "u"]);
}

#[test]
fn terminals_fold_their_references_into_one_literal_per_alternative() {
    let built = build("start: C\nA: \"a\"\nB: A \"b\"\nC: B | A");
    assert_eq!(
        term_body(&built, "B"),
        &Expr::Alternatives(vec![alt(vec![Expr::Literal(Pattern::re("ab"))])])
    );
    assert_eq!(
        term_body(&built, "C"),
        &Expr::Alternatives(vec![
            alt(vec![Expr::Literal(Pattern::re("ab"))]),
            alt(vec![Expr::Literal(Pattern::str("a"))]),
        ])
    );
    let grammar = compile(&built, &["start"]).unwrap();
    let c = &grammar.terminals[grammar.terminal_id("C").unwrap()];
    assert_eq!(c.regexp.as_deref(), Some("(?:ab|a)"));
}

#[test]
fn an_extended_imported_terminal_sorts_its_new_alternatives_with_the_old() {
    let sources = Sources(vec![("lib.lark", "A: B \"b\" | \"c\"\nB: \"a\"\n")]);
    let built = build_with(
        "%import lib.A\n%extend A: \"xyz\" | \"q\"\nstart: A",
        &sources,
    )
    .unwrap();
    assert_eq!(term_names(&built), vec!["A"]);
    let grammar = compile(&built, &["start"]).unwrap();
    let a = &grammar.terminals[grammar.terminal_id("A").unwrap()];
    assert_eq!(a.regexp.as_deref(), Some("(?:xyz|ab|q|c)"));
}

#[test]
fn a_long_chain_of_terminal_references_resolves_without_growing() {
    let mut text = String::from("start: T3000\nT0: \"a\"\n");
    for i in 1..=3000 {
        text.push_str(&format!("T{i}: T{}\n", i - 1));
    }
    let built = build(&text);
    assert_eq!(
        term_body(&built, "T3000"),
        &Expr::Alternatives(vec![alt(vec![Expr::Literal(Pattern::str("a"))])])
    );
    let mut text = String::from("start: T0\n");
    for i in 0..3000 {
        text.push_str(&format!("T{i}: T{}\n", i + 1));
    }
    text.push_str("T3000: \"a\"\n");
    build(&text);
}

#[test]
fn repeat_suffixes() {
    assert_eq!(repeat_suffix(0, Some(1)), "?");
    assert_eq!(repeat_suffix(0, None), "*");
    assert_eq!(repeat_suffix(1, None), "+");
    assert_eq!(repeat_suffix(3, Some(3)), "{3}");
    assert_eq!(repeat_suffix(1, Some(3)), "{1,3}");
}

#[test]
fn terminal_reference_errors() {
    assert!(
        err("start: A\nA: \"a\" start")
            .contains("Rules aren't allowed inside terminals (start in A)")
    );
    assert!(err("start: A\nA: \"a\" B").contains("Terminal used but not defined: B"));
    assert!(err("start: A\nA: \"a\" A").contains("Recursion in terminal 'A'"));
    assert!(err("start: A\nA: \"a\" B\nB: \"b\" A").contains("Recursion in terminal"));
    assert!(err("start: A\nA: \"a\" B\n%declare B").contains("uses declared terminal B"));
}

#[test]
fn a_terminal_doubling_at_every_link_of_a_chain_is_capped() {
    let chain = |links: usize| {
        let mut text = String::from("start: T0\n");
        for i in 0..links {
            text.push_str(&format!("T{i}: T{} T{}\n", i + 1, i + 1));
        }
        text.push_str(&format!("T{links}: \"a\"\n"));
        text
    };
    build(&chain(16));
    assert_eq!(
        err(&chain(17)),
        "Terminal T0 has a pattern longer than 65536 characters once the terminals it uses are inlined"
    );
}

#[test]
fn imports_from_bundled_grammars() {
    let built = build("start: INT WS\n%import common.INT\n%import common.WS");
    assert_eq!(term_names(&built), vec!["INT", "WS"]);
    let built = build("start: SIGNED_NUMBER\n%import common.SIGNED_NUMBER");
    assert_eq!(term_names(&built), vec!["SIGNED_NUMBER"]);
    assert!(
        err("start: \"a\"\n%import nonexistent.X")
            .contains("Could not find grammar `nonexistent.lark` to import")
    );
}

#[test]
fn imports_mangle_dependencies_and_keep_only_what_is_needed() {
    let sources = Sources(vec![(
        "lib.lark",
        "a: b _c D\nb: \"b\"\n_c: \"c\"\nD: E \"d\"\nE: \"e\"\nunused: \"u\"\n%ignore \" \"\n",
    )]);
    let built = build_with("%import lib.a\nstart: a", &sources).unwrap();
    assert_eq!(rule_names(&built), vec!["a", "lib__b", "_lib__c", "start"]);
    assert_eq!(term_names(&built), vec!["lib__D"]);
    assert!(built.ignore.is_empty());
}

#[test]
fn import_forms() {
    let sources = Sources(vec![("dir/lib.lark", "a: B\nB: \"b\"\nC: \"c\"\n")]);
    let built = build_with(
        "%import dir.lib (a, C)\n%import dir.lib.B -> BEE\nstart: a C BEE",
        &sources,
    )
    .unwrap();
    assert_eq!(rule_names(&built), vec!["a", "start"]);
    assert_eq!(term_names(&built), vec!["BEE", "C"]);
}

#[test]
fn relative_imports_resolve_against_the_importing_file() {
    let sources = Sources(vec![
        ("sub/main.lark", "%import .lib.a\nb: a\n"),
        ("sub/lib.lark", "a: \"a\"\n"),
    ]);
    let built = build_with("%import sub.main.b\nstart: b", &sources).unwrap();
    assert_eq!(rule_names(&built), vec!["sub__main__a", "b", "start"]);
    let built = build_with(
        "%import .lib.a\nstart: a",
        &Sources(vec![("lib.lark", "a: \"a\"\n")]),
    )
    .unwrap();
    assert_eq!(rule_names(&built), vec!["a", "start"]);
}

#[test]
fn no_bundled_grammar_stands_in_for_a_relative_import() {
    assert_eq!(
        err("%import .common.INT\nstart: INT"),
        "Could not find grammar `common.lark` to import"
    );
    assert_eq!(
        err("%import common.INT\n%import .common.WS\nstart: INT WS"),
        "Could not find grammar `common.lark` to import"
    );
    let sources = Sources(vec![("common.lark", "INT: \"7\"\n")]);
    let built = build_with("%import .common.INT\nstart: INT", &sources).unwrap();
    assert_eq!(term_names(&built), vec!["INT"]);
}

#[test]
fn a_relative_import_of_a_file_the_bundled_grammar_stood_in_for_is_an_error() {
    let mut files = GrammarFiles::new(String::from(
        "%import common.INT\n%import a.x\nstart: INT x\n",
    ))
    .unwrap();
    assert_eq!(paths(&files), vec!["common.lark", "a.lark"]);
    files.provide(0, None).unwrap();
    assert_eq!(
        files
            .provide(1, Some(String::from("%import .common.WS\nx: WS\n")))
            .unwrap_err()
            .message,
        "In grammar `a.lark`: Could not find grammar `common.lark` to import"
    );
}

#[test]
fn a_relative_import_of_a_file_not_yet_provided_keeps_the_bundled_grammar_out() {
    let mut files = GrammarFiles::new(String::from(
        "%import common.INT\n%import a.x\nstart: INT x\n",
    ))
    .unwrap();
    files
        .provide(1, Some(String::from("%import .common.WS\nx: WS\n")))
        .unwrap();
    assert_eq!(
        files.provide(0, None).unwrap_err().message,
        "Could not find grammar `common.lark` to import"
    );
}

#[test]
fn the_bundled_grammars_make_no_relative_imports() {
    for path in ["common.lark", "lark.lark", "python.lark", "unicode.lark"] {
        for statement in parse_grammar(bundled_grammar(path).unwrap()).unwrap() {
            if let Statement::Import(stmt) = statement {
                assert!(!stmt.relative, "{path}");
            }
        }
    }
}

#[test]
fn nested_imports_compose_mangles() {
    let sources = Sources(vec![
        ("outer.lark", "%import inner.x\na: x y\ny: \"y\"\n"),
        ("inner.lark", "x: z\nz: \"z\"\n"),
    ]);
    let built = build_with("%import outer.a\nstart: a", &sources).unwrap();
    assert_eq!(
        rule_names(&built),
        vec!["outer__x", "outer__inner__z", "a", "outer__y", "start"]
    );
}

#[test]
fn import_conflicts() {
    let sources = Sources(vec![
        ("lib.lark", "a: \"a\"\n"),
        ("lib2.lark", "a: \"b\"\n"),
    ]);
    assert!(
        build_with("%import lib.a\n%import lib2.a\nstart: a", &sources)
            .unwrap_err()
            .message
            .contains("Cannot import 'a' from 'lib2.lark': Symbol already defined.")
    );
    assert!(
        build_with("a: \"x\"\n%import lib.a\nstart: a", &sources)
            .unwrap_err()
            .message
            .contains("Rule 'a' defined more than once")
    );
}

#[test]
fn imported_declare_and_override_are_mangled() {
    let sources = Sources(vec![(
        "lib.lark",
        "a: B\n%declare B\nc: \"c\"\n%override c: \"d\"\n",
    )]);
    let built = build_with("%import lib (a, c)\nstart: a c", &sources).unwrap();
    assert_eq!(term_names(&built), vec!["lib__B"]);
    assert_eq!(built.term_defs[0].1, None);
}

#[test]
fn mangle_rules() {
    let mangle = Mangle {
        prefix: "a__b".to_string(),
        aliases: vec![("X".to_string(), "Y".to_string())],
        base: Some(Box::new(Mangle {
            prefix: "outer".to_string(),
            aliases: Vec::new(),
            base: None,
        })),
    };
    assert_eq!(mangle.apply("X"), "outer__Y");
    assert_eq!(mangle.apply("z"), "outer__a__b__z");
    assert_eq!(mangle.apply("_z"), "_outer__a__b__z");
}

#[test]
fn find_used_symbols_includes_templates_and_arguments() {
    let built = build("start: t{A, b} c\nt{x, y}: x y\nA: \"a\"\nb: \"b\"\nc: \"c\"");
    let symbols = find_used_symbols(&built.rule_defs[0].2);
    assert_eq!(symbols, vec!["t", "A", "b", "c"]);
}

#[test]
fn imported_rules_bring_their_templates_along_mangled() {
    let sources = Sources(vec![("lib.lark", "tmpl{x}: x \"a\"\nrule: tmpl{\"b\"}\n")]);
    let built = build_with("%import .lib.rule\nstart: rule\n", &sources).unwrap();
    let names = rule_names(&built);
    assert!(names.contains(&"rule"), "{names:?}");
    assert!(names.contains(&"lib__tmpl"), "{names:?}");
}

#[test]
fn imported_rules_may_reference_what_their_grammar_lacks() {
    let sources = Sources(vec![("lib.lark", "rule: other\n")]);
    let err = build_with("%import .lib.rule\nstart: rule\n", &sources).unwrap_err();
    assert!(err.message.contains("lib__other"), "{}", err.message);
}

#[test]
fn a_later_import_alias_of_the_same_name_replaces_the_earlier() {
    let built = build("%import common.INT -> A\n%import common.INT -> B\nstart: B\n");
    let names = term_names(&built);
    assert!(names.contains(&"B"), "{names:?}");
    assert!(!names.contains(&"A"), "{names:?}");
    assert!(err("%import common.INT -> A\n%import common.INT -> B\nstart: A\n").contains("'A'"));
}

#[test]
fn undefined_symbols_are_found_inside_repeats_and_template_arguments() {
    let message = err("start: \"x\" UNDEF*\n");
    assert!(
        message.contains("Terminal 'UNDEF' used but not defined"),
        "{message}"
    );
    let message = err("start: tmpl{UNDEF}\ntmpl{a}: a\n");
    assert!(
        message.contains("Terminal 'UNDEF' used but not defined"),
        "{message}"
    );
    let message = err("start: tmpl{\"q\"} nope\ntmpl{a}: a\n");
    assert!(
        message.contains("Rule 'nope' used but not defined"),
        "{message}"
    );
}

#[test]
fn a_rule_imported_under_a_terminal_name_is_not_a_terminal() {
    let import = "%import python.starguard -> STARGUARD\n";
    assert_eq!(
        err(&format!("{import}start: STARGUARD\n")),
        "Rule 'STARGUARD' used as a terminal (in rule start)"
    );
    assert_eq!(
        err(&format!("{import}start: \"a\"\n%ignore STARGUARD\n")),
        "Rule 'STARGUARD' marked to ignore, but only terminals can be ignored"
    );
    assert_eq!(
        err(&format!("{import}start: A\nA: STARGUARD \"x\"\n")),
        "Terminal used but not defined: STARGUARD"
    );
    assert_eq!(
        err("%import python.lambda_kwparams -> LK\nstart: A\nA: LK\n"),
        "Terminal used but not defined: LK"
    );
}

#[test]
fn a_terminal_imported_under_a_rule_name_is_not_a_rule() {
    assert_eq!(
        err("%import common.INT -> int\nstart: int\n"),
        "Terminal 'int' used as a rule (in rule start)"
    );
    assert_eq!(
        err("%import common.INT -> int\nstart: int{\"a\"}\n"),
        "Terminal 'int' used as a template (in rule start)"
    );
}

#[test]
fn an_imported_rule_used_as_a_terminal_is_rejected_before_its_own_references_are_checked() {
    let sources = Sources(vec![("g.lark", "foo: undefined_rule\n")]);
    let err = build_with("%import .g.foo -> FOO\nstart: A\nA: FOO\n", &sources).unwrap_err();
    assert_eq!(err.message, "Terminal used but not defined: FOO");
}
