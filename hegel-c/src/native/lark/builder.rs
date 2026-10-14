//! Assembling a grammar from its statements: Lark's `GrammarBuilder`.
//! Handles `%import` (with Lark's name mangling and pruning of the
//! imported grammar to what was asked for), `%override`, `%extend`,
//! `%ignore`, `%declare`, template parameters, terminal references inside
//! terminals, and the validation Lark performs before compiling.

use super::ast::{
    Alternative, Definition as DefStmt, Expr, ImportNames, ImportStmt, Pattern, RuleModifiers,
    Statement, Symbol,
};
use super::compile::parse_terminal_pattern;
use super::escape::pattern_to_regexp;
use super::parser::parse_grammar;
use super::{GrammarError, GrammarResult};
use crate::native::re::width::get_width;
use crate::native::{HashMap, HashSet};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Where `%import`ed grammars come from: a lookup of `grammar_path`
/// (a relative file path such as `common.lark` or `dir/sub.lark`) given
/// the directory of the importing grammar when that is known. Returns the
/// path the grammar was found at and its text. `Ok(None)` permits bundled
/// fallback; `Err` aborts compilation.
pub trait ImportSource {
    fn load(
        &self,
        base_path: Option<&str>,
        grammar_path: &str,
    ) -> GrammarResult<Option<(String, String)>>;
}

/// An import source with no grammars: only the bundled ones resolve.
pub struct NoImports;

impl ImportSource for NoImports {
    fn load(
        &self,
        _base_path: Option<&str>,
        _grammar_path: &str,
    ) -> GrammarResult<Option<(String, String)>> {
        Ok(None)
    }
}

/// The grammars Lark 1.3.1 ships in `lark/grammars/`, copied verbatim
/// under `grammars/` with Lark's licence, reachable as `%import common.X`
/// and so on.
pub fn bundled_grammar(grammar_path: &str) -> Option<&'static str> {
    match grammar_path {
        "common.lark" => Some(include_str!("grammars/common.lark")),
        "lark.lark" => Some(include_str!("grammars/lark.lark")),
        "python.lark" => Some(include_str!("grammars/python.lark")),
        "unicode.lark" => Some(include_str!("grammars/unicode.lark")),
        _ => None,
    }
}

/// Options recorded on a rule definition. Nothing here reads them: a
/// rule's priority and tree-shaping modifiers only matter to a parser
/// building trees, and they are kept so that the loader accepts every
/// grammar Lark does and reports what it parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleOptions {
    pub modifiers: RuleModifiers,
    pub priority: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefOptions {
    Rule(RuleOptions),
    Terminal { priority: i64 },
}

/// A rule or terminal definition as the builder holds it: `tree` is
/// `None` for a `%declare`d terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub is_term: bool,
    pub params: Vec<String>,
    pub tree: Option<Expr>,
    pub options: DefOptions,
}

/// Lark's import-time renaming: `X` imported from `a.b` becomes `a__b__X`
/// (`_X` becomes `_a__b__X`), except for the names the import asked for,
/// which take their alias; a nested import composes with the importer's
/// own mangle.
#[derive(Clone)]
pub struct Mangle {
    prefix: String,
    aliases: Vec<(String, String)>,
    base: Option<Box<Mangle>>,
}

impl Mangle {
    pub fn apply(&self, s: &str) -> String {
        let mut out = match self.aliases.iter().find(|(from, _)| from == s) {
            Some((_, to)) => to.clone(),
            None => match s.strip_prefix('_') {
                Some(rest) => format!("_{}__{rest}", self.prefix),
                None => format!("{}__{s}", self.prefix),
            },
        };
        if let Some(base) = &self.base {
            out = base.apply(&out);
        }
        out
    }
}

/// The grammar as built: Lark's `Grammar(rule_defs, term_defs, ignore)`,
/// ready for [`super::compile`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltGrammar {
    /// `(name, params, body, options)` in definition order.
    pub rule_defs: Vec<(String, Vec<String>, Expr, RuleOptions)>,
    /// `(name, body, priority)` in definition order; `body` is `None` for a
    /// `%declare`d terminal.
    pub term_defs: Vec<(String, Option<Expr>, i64)>,
    pub ignore: Vec<String>,
}

pub struct GrammarBuilder<'s> {
    source: &'s dyn ImportSource,
    /// The paths of the grammars whose imports led to this one, the
    /// outermost first: an import of a path already here is a cycle.
    import_stack: Vec<String>,
    names: Vec<String>,
    definitions: HashMap<String, Definition>,
    ignore_names: Vec<String>,
}

fn type_name(is_term: bool) -> &'static str {
    if is_term { "terminal" } else { "rule" }
}

fn type_title(is_term: bool) -> &'static str {
    if is_term { "Terminal" } else { "Rule" }
}

fn mangle_expr(expr: &mut Expr, mangle: &Mangle) {
    match expr {
        Expr::Alternatives(alts) => {
            for alt in alts {
                for item in &mut alt.items {
                    mangle_expr(item, mangle);
                }
            }
        }
        Expr::Symbol(Symbol::Terminal(name) | Symbol::Rule(name)) => {
            *name = mangle.apply(name);
        }
        Expr::Literal(_) => {}
        Expr::Repeat { inner, .. } => mangle_expr(inner, mangle),
        Expr::Template { name, args } => {
            *name = mangle.apply(name);
            for arg in args {
                mangle_expr(arg, mangle);
            }
        }
    }
}

fn collect_symbols<'e>(expr: &'e Expr, out: &mut Vec<&'e str>) {
    match expr {
        Expr::Alternatives(alts) => {
            for alt in alts {
                for item in &alt.items {
                    collect_symbols(item, out);
                }
            }
        }
        Expr::Symbol(symbol) => out.push(symbol.name()),
        Expr::Literal(_) => {}
        Expr::Repeat { inner, .. } => collect_symbols(inner, out),
        Expr::Template { name, args } => {
            out.push(name);
            for arg in args {
                collect_symbols(arg, out);
            }
        }
    }
}

/// Every symbol name referenced by `expr`, including template names.
pub fn find_used_symbols(expr: &Expr) -> Vec<&str> {
    let mut out = Vec::new();
    collect_symbols(expr, &mut out);
    out
}

fn collect_templates<'e>(expr: &'e Expr, out: &mut Vec<(&'e str, usize)>) {
    match expr {
        Expr::Alternatives(alts) => {
            for alt in alts {
                for item in &alt.items {
                    collect_templates(item, out);
                }
            }
        }
        Expr::Symbol(_) | Expr::Literal(_) => {}
        Expr::Repeat { inner, .. } => collect_templates(inner, out),
        Expr::Template { name, args } => {
            out.push((name, args.len()));
            for arg in args {
                collect_templates(arg, out);
            }
        }
    }
}

/// The symbols an expression references directly (not through templates).
fn symbol_references<'e>(expr: &'e Expr, out: &mut Vec<&'e Symbol>) {
    match expr {
        Expr::Alternatives(alts) => {
            for alt in alts {
                for item in &alt.items {
                    symbol_references(item, out);
                }
            }
        }
        Expr::Symbol(symbol) => out.push(symbol),
        Expr::Literal(_) | Expr::Template { .. } => {}
        Expr::Repeat { inner, .. } => symbol_references(inner, out),
    }
}

/// The most characters a terminal's regex may have once the terminals
/// it references are folded into it. A chain of terminals each using the
/// one before it twice doubles in length at every link, and Lark builds
/// the whole of it; here it is an error instead.
const MAX_TERMINAL_PATTERN_LEN: usize = 1 << 16;

/// How deeply imports may lead from grammar to grammar. A cycle is
/// caught by its path, but a source may answer every path with a new
/// grammar that imports another.
const MAX_IMPORT_NESTING: usize = 64;

fn contains_template(expr: &Expr) -> bool {
    match expr {
        Expr::Alternatives(alts) => alts.iter().any(|a| a.items.iter().any(contains_template)),
        Expr::Symbol(_) | Expr::Literal(_) => false,
        Expr::Repeat { inner, .. } => contains_template(inner),
        Expr::Template { .. } => true,
    }
}

/// The widths of a pattern.
fn pattern_widths(name: &str, pattern: &Pattern) -> GrammarResult<(u64, Option<u64>)> {
    Ok(get_width(&parse_terminal_pattern(name, pattern)?.pattern))
}

/// The regex quantifier for a repetition. The parser builds an unbounded
/// one only from `*` or `+`.
fn repeat_suffix(min: u32, max: Option<u32>) -> String {
    match (min, max) {
        (0, Some(1)) => "?".to_string(),
        (0, None) => "*".to_string(),
        (_, None) => "+".to_string(),
        (n, Some(m)) if n == m => format!("{{{n}}}"),
        (n, Some(m)) => format!("{{{n},{m}}}"),
    }
}

/// Lark's `TerminalTreeToPattern`: fold a terminal's body into one
/// pattern, with each terminal it references standing for that
/// terminal's own pattern from `resolved`. `name` is for diagnostics.
fn terminal_tree_to_pattern(
    name: &str,
    expr: &Expr,
    resolved: &HashMap<String, Pattern>,
) -> GrammarResult<Pattern> {
    match expr {
        Expr::Alternatives(alts) => {
            join_alternatives(name, alternative_patterns(name, alts, resolved)?)
        }
        Expr::Literal(pattern) => Ok(pattern.clone()),
        Expr::Symbol(Symbol::Terminal(reference)) => Ok(resolved[reference.as_str()].clone()),
        Expr::Repeat { inner, min, max } => {
            let inner = terminal_tree_to_pattern(name, inner, resolved)?;
            Ok(Pattern::re(format!(
                "(?:{}){}",
                pattern_to_regexp(&inner),
                repeat_suffix(*min, *max)
            ))
            .with_flags(&inner.flags))
        }
        Expr::Symbol(Symbol::Rule(_)) | Expr::Template { .. } => {
            unreachable!("rules and templates inside terminals are rejected before folding")
        }
    }
}

/// The pattern of each alternative of a terminal body.
fn alternative_patterns(
    name: &str,
    alts: &[Alternative],
    resolved: &HashMap<String, Pattern>,
) -> GrammarResult<Vec<Pattern>> {
    if alts.iter().any(|a| a.alias.is_some()) {
        return Err(GrammarError::new(
            "Aliasing not allowed in terminals (You used -> in the wrong place)",
        ));
    }
    alts.iter()
        .map(|alt| sequence_to_pattern(name, &alt.items, resolved))
        .collect()
}

/// One pattern matching any of `patterns`, ordered as Lark orders them:
/// the one matching the longest strings first.
fn join_alternatives(name: &str, mut patterns: Vec<Pattern>) -> GrammarResult<Pattern> {
    if patterns.len() == 1 {
        return Ok(patterns.pop().unwrap());
    }
    let mut keyed = Vec::with_capacity(patterns.len());
    for pattern in patterns {
        let (min, max) = pattern_widths(name, &pattern)?;
        keyed.push((
            max.unwrap_or(u64::MAX),
            min,
            pattern.value.chars().count(),
            pattern,
        ));
    }
    keyed.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    let joined: Vec<String> = keyed.iter().map(|k| pattern_to_regexp(&k.3)).collect();
    Ok(Pattern::re(format!("(?:{})", joined.join("|"))))
}

fn sequence_to_pattern(
    name: &str,
    items: &[Expr],
    resolved: &HashMap<String, Pattern>,
) -> GrammarResult<Pattern> {
    match items {
        [] => Ok(Pattern::str("")),
        [item] => terminal_tree_to_pattern(name, item, resolved),
        _ => {
            let mut joined = String::new();
            for item in items {
                let pattern = terminal_tree_to_pattern(name, item, resolved)?;
                joined.push_str(&pattern_to_regexp(&pattern));
            }
            Ok(Pattern::re(joined))
        }
    }
}

/// The pattern of a terminal body as [`GrammarBuilder`] leaves it: with
/// the terminals it referenced already folded in.
pub fn terminal_pattern(name: &str, body: &Expr) -> GrammarResult<Pattern> {
    terminal_tree_to_pattern(name, body, &HashMap::default())
}

impl<'s> GrammarBuilder<'s> {
    pub fn new(source: &'s dyn ImportSource) -> Self {
        GrammarBuilder {
            source,
            import_stack: Vec::new(),
            names: Vec::new(),
            definitions: HashMap::default(),
            ignore_names: Vec::new(),
        }
    }

    fn insert(&mut self, name: String, definition: Definition) {
        if !self.definitions.contains_key(&name) {
            self.names.push(name.clone());
        }
        self.definitions.insert(name, definition);
    }

    fn define(
        &mut self,
        name: String,
        is_term: bool,
        tree: Option<Expr>,
        params: Vec<String>,
        options: DefOptions,
        override_existing: bool,
    ) -> GrammarResult<()> {
        if self.definitions.contains_key(&name) {
            if !override_existing {
                return Err(GrammarError::new(format!(
                    "{} '{name}' defined more than once",
                    type_title(is_term)
                )));
            }
        } else if override_existing {
            return Err(GrammarError::new(format!(
                "Cannot override a nonexisting {} {name}",
                type_name(is_term)
            )));
        }
        self.insert(
            name,
            Definition {
                is_term,
                params,
                tree,
                options,
            },
        );
        Ok(())
    }

    fn extend(
        &mut self,
        name: String,
        is_term: bool,
        tree: Expr,
        params: Vec<String>,
    ) -> GrammarResult<()> {
        let Some(existing) = self.definitions.get_mut(&name) else {
            return Err(GrammarError::new(format!(
                "Can't extend {} {name} as it wasn't defined before",
                type_name(is_term)
            )));
        };
        if existing.is_term != is_term {
            return Err(GrammarError::new(format!(
                "Cannot extend {} {name} - one is a terminal, while the other is not.",
                type_name(is_term)
            )));
        }
        if existing.params != params {
            return Err(GrammarError::new(format!(
                "Cannot extend {} with different parameters: {name}",
                type_name(is_term)
            )));
        }
        let Some(base) = &mut existing.tree else {
            return Err(GrammarError::new(format!(
                "Can't extend {} {name} - it is abstract.",
                type_name(is_term)
            )));
        };
        let (Expr::Alternatives(alts), Expr::Alternatives(extension)) = (base, tree) else {
            unreachable!("definition bodies are always alternatives")
        };
        alts.splice(0..0, extension);
        Ok(())
    }

    fn ignore(&mut self, expr: Expr) {
        if let Expr::Alternatives(alts) = &expr {
            if alts.len() == 1 && alts[0].items.len() == 1 && alts[0].alias.is_none() {
                if let Expr::Symbol(Symbol::Terminal(name)) = &alts[0].items[0] {
                    self.ignore_names.push(name.clone());
                    return;
                }
            }
        }
        let name = format!("__IGNORE_{}", self.ignore_names.len());
        self.ignore_names.push(name.clone());
        self.insert(
            name,
            Definition {
                is_term: true,
                params: Vec::new(),
                tree: Some(expr),
                options: DefOptions::Terminal { priority: 0 },
            },
        );
    }

    fn declare(&mut self, symbols: &[Symbol], mangle: Option<&Mangle>) -> GrammarResult<()> {
        for symbol in symbols {
            let Symbol::Terminal(name) = symbol else {
                return Err(GrammarError::new(format!(
                    "Expecting terminal name to follow %declare, but got rule name '{}'",
                    symbol.name()
                )));
            };
            let name = match mangle {
                Some(m) => m.apply(name),
                None => name.clone(),
            };
            self.define(
                name,
                true,
                None,
                Vec::new(),
                DefOptions::Terminal { priority: 0 },
                false,
            )?;
        }
        Ok(())
    }

    fn unpack_definition(
        definition: DefStmt,
        mangle: Option<&Mangle>,
    ) -> (String, bool, Expr, Vec<String>, DefOptions) {
        let (name, is_term, mut tree, params, options) = match definition {
            DefStmt::Rule(rule) => (
                rule.name,
                false,
                rule.body,
                rule.params,
                DefOptions::Rule(RuleOptions {
                    modifiers: rule.modifiers,
                    priority: rule.priority,
                }),
            ),
            DefStmt::Terminal(term) => (
                term.name,
                true,
                term.body,
                Vec::new(),
                DefOptions::Terminal {
                    priority: term.priority,
                },
            ),
        };
        match mangle {
            Some(m) => {
                mangle_expr(&mut tree, m);
                (
                    m.apply(&name),
                    is_term,
                    tree,
                    params.iter().map(|p| m.apply(p)).collect(),
                    options,
                )
            }
            None => (name, is_term, tree, params, options),
        }
    }

    fn do_import(
        &mut self,
        stmt: &ImportStmt,
        aliases: Vec<(String, String)>,
        base_path: Option<&str>,
        base_mangle: Option<&Mangle>,
    ) -> GrammarResult<()> {
        if self.import_stack.len() >= MAX_IMPORT_NESTING {
            return Err(GrammarError::new(format!(
                "Grammar imports nest more than {MAX_IMPORT_NESTING} levels deep"
            )));
        }
        let mangle = Mangle {
            prefix: stmt.path.join("__"),
            aliases,
            base: base_mangle.map(|m| Box::new(m.clone())),
        };
        let grammar_path = format!("{}.lark", stmt.path.join("/"));
        let loaded = self.source.load(base_path, &grammar_path)?.or_else(|| {
            bundled_grammar(&grammar_path).map(|text| (grammar_path.clone(), text.to_string()))
        });
        let Some((joined_path, text)) = loaded else {
            return Err(GrammarError::new(format!(
                "Could not find grammar `{grammar_path}` to import"
            )));
        };
        if self.import_stack.contains(&joined_path) {
            return Err(GrammarError::new(format!(
                "Circular grammar import: {} -> {joined_path}",
                self.import_stack.join(" -> ")
            )));
        }
        let mut imported = GrammarBuilder::new(self.source);
        imported.import_stack = self.import_stack.clone();
        imported.import_stack.push(joined_path.clone());
        imported.load_grammar(&text, Some(&joined_path), Some(&mangle))?;
        let used: Vec<String> = mangle
            .aliases
            .iter()
            .map(|(k, _)| mangle.apply(k))
            .collect();
        imported.remove_unused(&used);
        for name in imported.names {
            if self.definitions.contains_key(&name) {
                return Err(GrammarError::new(format!(
                    "Cannot import '{name}' from '{grammar_path}': Symbol already defined."
                )));
            }
            let definition = imported.definitions.remove(&name).unwrap();
            self.insert(name, definition);
        }
        Ok(())
    }

    fn remove_unused(&mut self, used: &[String]) {
        let mut keep: HashSet<String> = HashSet::default();
        let mut queue: Vec<String> = used.to_vec();
        while let Some(name) = queue.pop() {
            if !keep.insert(name.clone()) {
                continue;
            }
            let Some(definition) = self.definitions.get(&name) else {
                continue;
            };
            if definition.is_term {
                continue;
            }
            if let Some(tree) = &definition.tree {
                for symbol in find_used_symbols(tree) {
                    if !definition.params.iter().any(|p| p == symbol) {
                        queue.push(symbol.to_string());
                    }
                }
            }
        }
        self.names.retain(|n| keep.contains(n));
        self.definitions.retain(|n, _| keep.contains(n));
    }

    /// Lark's `resolve_term_references`, which inlines every terminal
    /// referenced from another terminal's body: here each terminal's body
    /// is folded to its patterns instead, which is what the inlined trees
    /// come to and keeps a chain of references linear. Rejects rules
    /// inside terminals, undefined terminals and recursive terminals.
    fn resolve_term_references(&mut self) -> GrammarResult<()> {
        let term_names: Vec<String> = self
            .names
            .iter()
            .filter(|n| self.definitions[*n].is_term)
            .cloned()
            .collect();
        for name in &term_names {
            let Some(tree) = &self.definitions[name].tree else {
                continue;
            };
            let mut symbols = Vec::new();
            symbol_references(tree, &mut symbols);
            for symbol in symbols {
                let Symbol::Terminal(reference) = symbol else {
                    return Err(GrammarError::new(format!(
                        "Rules aren't allowed inside terminals ({} in {name})",
                        symbol.name()
                    )));
                };
                match self.definitions.get(reference) {
                    Some(d) if d.is_term && d.tree.is_none() => {
                        return Err(GrammarError::new(format!(
                            "Terminal {name} uses declared terminal {reference}, which has no definition to inline"
                        )));
                    }
                    Some(d) if d.is_term => {}
                    _ => {
                        return Err(GrammarError::new(format!(
                            "Terminal used but not defined: {reference}"
                        )));
                    }
                }
            }
        }
        let mut patterns: HashMap<String, Pattern> = HashMap::default();
        for name in &term_names {
            self.resolve_terminal(name, &mut patterns)?;
        }
        Ok(())
    }

    /// The terminals the body of the terminal `name` references.
    fn terminal_references(&self, name: &str) -> Vec<String> {
        let mut references = Vec::new();
        symbol_references(
            self.definitions[name].tree.as_ref().unwrap(),
            &mut references,
        );
        references
            .into_iter()
            .map(|symbol| symbol.name().to_string())
            .collect()
    }

    /// Fold the terminal `name`, and first the terminals it references,
    /// to their patterns: `patterns` collects the whole pattern of each,
    /// and the definition's body becomes one literal per alternative,
    /// so that a later `%extend` adds its alternatives alongside them.
    fn resolve_terminal(
        &mut self,
        name: &str,
        patterns: &mut HashMap<String, Pattern>,
    ) -> GrammarResult<()> {
        if patterns.contains_key(name) || self.definitions[name].tree.is_none() {
            return Ok(());
        }
        let mut stack = alloc::vec![(name.to_string(), self.terminal_references(name), 0)];
        while let Some((_, references, next)) = stack.last_mut() {
            if let Some(reference) = references.get(*next).cloned() {
                *next += 1;
                if patterns.contains_key(&reference) {
                    continue;
                }
                if stack.iter().any(|(n, _, _)| *n == reference) {
                    return Err(GrammarError::new(format!(
                        "Recursion in terminal '{reference}' (recursion is only allowed in rules, not terminals)"
                    )));
                }
                let references = self.terminal_references(&reference);
                stack.push((reference, references, 0));
                continue;
            }
            let (current, _, _) = stack.pop().unwrap();
            let tree = self.definitions[&current].tree.as_ref().unwrap();
            if contains_template(tree) {
                return Err(GrammarError::new(format!(
                    "Templates cannot be used inside terminals ({current})"
                )));
            }
            if tree.is_single_empty() {
                return Err(GrammarError::new(format!(
                    "Terminals cannot be empty ({current})"
                )));
            }
            let Expr::Alternatives(alts) = tree else {
                unreachable!("definition bodies are always alternatives")
            };
            let alternatives = alternative_patterns(&current, alts, patterns)?;
            let whole = join_alternatives(&current, alternatives.clone())?;
            if whole.value.chars().count() > MAX_TERMINAL_PATTERN_LEN {
                return Err(GrammarError::new(format!(
                    "Terminal {current} has a pattern longer than {MAX_TERMINAL_PATTERN_LEN} characters once the terminals it uses are inlined"
                )));
            }
            let folded = alternatives
                .into_iter()
                .map(|pattern| Alternative {
                    items: alloc::vec![Expr::Literal(pattern)],
                    alias: None,
                })
                .collect();
            self.definitions.get_mut(&current).unwrap().tree = Some(Expr::Alternatives(folded));
            patterns.insert(current, whole);
        }
        Ok(())
    }

    /// Load one grammar's statements, importing what it imports first.
    /// `source_path` is where the text came from, if it came from a file:
    /// relative imports resolve against its directory. `mangle` is set
    /// when this grammar is itself being imported.
    pub fn load_grammar(
        &mut self,
        text: &str,
        source_path: Option<&str>,
        mangle: Option<&Mangle>,
    ) -> GrammarResult<()> {
        let statements = parse_grammar(text)?;
        let base_path = source_path.map(|p| match p.rfind(['/', '\\']) {
            Some(i) => p[..i].to_string(),
            None => String::new(),
        });
        let mut imports: Vec<(&ImportStmt, Vec<(String, String)>)> = Vec::new();
        for statement in &statements {
            let Statement::Import(stmt) = statement else {
                continue;
            };
            let aliases: Vec<(String, String)> = match &stmt.names {
                ImportNames::Single { name, alias } => {
                    alloc::vec![(name.clone(), alias.clone().unwrap_or_else(|| name.clone()))]
                }
                ImportNames::Multi(names) => names.iter().map(|n| (n.clone(), n.clone())).collect(),
            };
            match imports
                .iter_mut()
                .find(|(s, _)| s.path == stmt.path && s.relative == stmt.relative)
            {
                Some((_, existing)) => {
                    for (from, to) in aliases {
                        match existing.iter_mut().find(|(f, _)| *f == from) {
                            Some(entry) => entry.1 = to,
                            None => existing.push((from, to)),
                        }
                    }
                }
                None => imports.push((stmt, aliases)),
            }
        }
        for (stmt, aliases) in imports {
            let base = if stmt.relative {
                base_path.as_deref()
            } else {
                None
            };
            self.do_import(stmt, aliases, base, mangle)?;
        }
        for statement in statements {
            match statement {
                Statement::Define(definition) => {
                    let (name, is_term, tree, params, options) =
                        Self::unpack_definition(definition, mangle);
                    self.define(name, is_term, Some(tree), params, options, false)?;
                }
                Statement::Override(definition) => {
                    let (name, is_term, tree, params, options) =
                        Self::unpack_definition(definition, mangle);
                    self.define(name, is_term, Some(tree), params, options, true)?;
                }
                Statement::Extend(definition) => {
                    let (name, is_term, tree, params, _) =
                        Self::unpack_definition(definition, mangle);
                    self.extend(name, is_term, tree, params)?;
                }
                Statement::Ignore(expr) => {
                    if mangle.is_none() {
                        self.ignore(expr);
                    }
                }
                Statement::Declare(symbols) => self.declare(&symbols, mangle)?,
                Statement::Import(_) => {}
            }
        }
        self.resolve_term_references()
    }

    /// Lark's `GrammarBuilder.validate`.
    pub fn validate(&self) -> GrammarResult<()> {
        for name in &self.names {
            let d = &self.definitions[name];
            for (i, p) in d.params.iter().enumerate() {
                if self.definitions.contains_key(p) {
                    return Err(GrammarError::new(format!(
                        "Template Parameter conflicts with rule {p} (in template {name})"
                    )));
                }
                if d.params[..i].contains(p) {
                    return Err(GrammarError::new(format!(
                        "Duplicate Template Parameter {p} (in template {name})"
                    )));
                }
            }
            let Some(tree) = &d.tree else {
                continue;
            };
            let mut templates = Vec::new();
            collect_templates(tree, &mut templates);
            for (sym, arity) in templates {
                if d.params.iter().any(|p| p == sym) {
                    continue;
                }
                let Some(target) = self.definitions.get(sym) else {
                    return Err(GrammarError::new(format!(
                        "Template '{sym}' used but not defined (in {} {name})",
                        type_name(d.is_term)
                    )));
                };
                if target.is_term {
                    return Err(GrammarError::new(format!(
                        "Terminal '{sym}' used as a template (in {} {name})",
                        type_name(d.is_term)
                    )));
                }
                if arity != target.params.len() {
                    return Err(GrammarError::new(format!(
                        "Wrong number of template arguments used for {sym} (expected {}, got {arity}) (in {} {name})",
                        target.params.len(),
                        type_name(d.is_term)
                    )));
                }
            }
            for sym in find_used_symbols(tree) {
                if !self.definitions.contains_key(sym) && !d.params.iter().any(|p| p == sym) {
                    let sym_is_term = tree_mentions_terminal(tree, sym);
                    return Err(GrammarError::new(format!(
                        "{} '{sym}' used but not defined (in {} {name})",
                        type_title(sym_is_term),
                        type_name(d.is_term)
                    )));
                }
            }
            let mut references = Vec::new();
            symbol_references(tree, &mut references);
            for symbol in references {
                let Some(target) = self.definitions.get(symbol.name()) else {
                    continue;
                };
                if target.is_term != symbol.is_terminal() {
                    return Err(GrammarError::new(format!(
                        "{} '{}' used as a {} (in {} {name})",
                        type_title(target.is_term),
                        symbol.name(),
                        type_name(symbol.is_terminal()),
                        type_name(d.is_term)
                    )));
                }
            }
        }
        let undefined: Vec<&String> = self
            .ignore_names
            .iter()
            .filter(|n| !self.definitions.contains_key(*n))
            .collect();
        if !undefined.is_empty() {
            let names: Vec<String> = undefined.iter().map(|n| format!("'{n}'")).collect();
            return Err(GrammarError::new(format!(
                "Terminals {{{}}} were marked to ignore but were not defined!",
                names.join(", ")
            )));
        }
        for name in &self.ignore_names {
            if !self.definitions[name].is_term {
                return Err(GrammarError::new(format!(
                    "Rule '{name}' marked to ignore, but only terminals can be ignored"
                )));
            }
        }
        Ok(())
    }

    pub fn build(self) -> GrammarResult<BuiltGrammar> {
        self.validate()?;
        let mut rule_defs = Vec::new();
        let mut term_defs = Vec::new();
        let mut definitions = self.definitions;
        for name in self.names {
            let d = definitions.remove(&name).unwrap();
            match d.options {
                DefOptions::Terminal { priority } => term_defs.push((name, d.tree, priority)),
                DefOptions::Rule(options) => {
                    rule_defs.push((name, d.params, d.tree.unwrap(), options));
                }
            }
        }
        Ok(BuiltGrammar {
            rule_defs,
            term_defs,
            ignore: self.ignore_names,
        })
    }
}

fn tree_mentions_terminal(tree: &Expr, name: &str) -> bool {
    tree_mentions(tree, name, true)
}

fn tree_mentions(tree: &Expr, name: &str, as_terminal: bool) -> bool {
    match tree {
        Expr::Alternatives(alts) => alts.iter().any(|alt| {
            alt.items
                .iter()
                .any(|i| tree_mentions(i, name, as_terminal))
        }),
        Expr::Symbol(symbol) => symbol.name() == name && symbol.is_terminal() == as_terminal,
        Expr::Literal(_) => false,
        Expr::Repeat { inner, .. } => tree_mentions(inner, name, as_terminal),
        Expr::Template { name: t, args } => {
            (!as_terminal && t == name) || args.iter().any(|a| tree_mentions(a, name, as_terminal))
        }
    }
}

/// Parse and build a grammar from its text.
pub fn load_grammar(
    text: &str,
    source_path: Option<&str>,
    source: &dyn ImportSource,
) -> GrammarResult<BuiltGrammar> {
    let mut builder = GrammarBuilder::new(source);
    builder.load_grammar(text, source_path, None)?;
    builder.build()
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/lark/builder_tests.rs"]
mod tests;
