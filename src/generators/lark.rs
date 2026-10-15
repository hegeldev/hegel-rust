use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::PathBuf;
use std::sync::OnceLock;

use super::generators::draw_and_print_value;
use super::strings::CharactersGenerator;
use super::{Generator, PrintableGenerator, TestCase, combine_labels, label_from_name};
use crate::ffi;
use crate::pretty::PrettyPrinter;
use crate::test_case::invalid_argument;

const LARK_LABEL: u64 = label_from_name("hegel.from_lark");
const ATTEMPT_LABEL: u64 = label_from_name("hegel.from_lark.attempt");
const DEFAULT_MAX_SIZE: usize = 8192;
const ATTEMPTS: usize = 3;

type ExplicitTerminal = dyn Generator<String> + Send + Sync + UnwindSafe + RefUnwindSafe;

/// A terminal whose values come from a generator of the user's rather
/// than from the grammar.
struct Explicit {
    name: String,
    generator: Box<ExplicitTerminal>,
}

/// The compiled grammar, with the marker that stands for each explicit
/// terminal in the strings drawn from it, in the order of `explicit`.
struct Compiled {
    grammar: ffi::Grammar,
    markers: Vec<String>,
}

/// One explicit terminal's generator as the test case draws it: a
/// forwarding wrapper, so the value gets the generator's own span.
struct Terminal<'a>(&'a ExplicitTerminal);

impl Generator<String> for Terminal<'_> {
    fn label(&self) -> u64 {
        self.0.label()
    }

    fn do_draw(&self, tc: &TestCase) -> String {
        self.0.do_draw(tc)
    }
}

/// Generator for strings matching a grammar written in Lark's syntax.
/// Created by [`from_lark()`].
///
/// The grammar is checked and compiled on the first draw; a grammar Lark
/// would reject, an undefined start rule, a cap below the smallest string
/// the grammar can produce, or an `explicit` generator for a terminal the
/// grammar does not use is reported then as an invalid argument.
pub struct LarkGenerator {
    text: String,
    start: Option<String>,
    explicit: Vec<Explicit>,
    alphabet: Option<CharactersGenerator>,
    max_size: Option<usize>,
    max_depth: Option<u32>,
    import_paths: Vec<PathBuf>,
    label: u64,
    handle: OnceLock<Compiled>,
}

impl LarkGenerator {
    /// Generate strings of the rule named `start` rather than of Lark's
    /// default start rule, `start`.
    pub fn start(mut self, start: &str) -> Self {
        self.handle = OnceLock::new();
        self.start = Some(start.to_string());
        self.relabel();
        self
    }

    /// Draw the terminal `name` from `generator` instead of from the
    /// terminal's own pattern. Required for a terminal the grammar
    /// `%declare`s without defining; any other terminal may be overridden.
    ///
    /// The grammar string is drawn with a marker standing for each
    /// occurrence of the terminal, then a value is drawn from `generator`
    /// for each marker, in order, and spliced in. The values are beyond
    /// the lexing checks the engine makes on the rest of the string: an
    /// `%ignore`d terminal goes on either side of each where the grammar
    /// has one, and it is the generator's part to draw values that read
    /// as the terminal where they land (a value under a quoted-string
    /// terminal should contain no quote, say). The values count toward
    /// [`max_size`](Self::max_size) like any other characters: a string
    /// they push over it is drawn again, with less of the cap for the
    /// grammar's own text, and a draw that cannot fit the values after a
    /// few tries rejects the test case.
    pub fn explicit<G>(mut self, name: &str, generator: G) -> Self
    where
        G: Generator<String> + Send + Sync + UnwindSafe + RefUnwindSafe + 'static,
    {
        self.handle = OnceLock::new();
        self.explicit.push(Explicit {
            name: name.to_string(),
            generator: Box::new(generator),
        });
        self.relabel();
        self
    }

    fn relabel(&mut self) {
        let mut labels = vec![
            LARK_LABEL,
            label_from_name(&self.text),
            label_from_name(self.start.as_deref().unwrap_or("start")),
        ];
        for explicit in &self.explicit {
            labels.push(label_from_name(&explicit.name));
            labels.push(explicit.generator.label());
        }
        self.label = combine_labels(&labels);
    }

    /// Constrain which characters terminals drawn from their patterns may
    /// contain, as [`RegexGenerator::alphabet`](super::RegexGenerator::alphabet)
    /// does for a single pattern.
    pub fn alphabet(mut self, alphabet: CharactersGenerator) -> Self {
        self.handle = OnceLock::new();
        self.alphabet = Some(alphabet);
        self
    }

    /// Set the maximum length in characters of the generated strings. The
    /// default is 8192, and the most allowed is 1048576.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.handle = OnceLock::new();
        self.max_size = Some(max_size);
        self
    }

    /// Set how deeply rules may nest in a generated string. The default is
    /// 200, and the most allowed is 500.
    pub fn max_depth(mut self, max_depth: u32) -> Self {
        self.handle = OnceLock::new();
        self.max_depth = Some(max_depth);
        self
    }

    /// Add a directory to search for `%import`ed grammars, as Lark's
    /// `import_paths` does. `%import a.b.X` reads `a/b.lark` from the first
    /// directory, in the order added, that has it. An import a grammar file
    /// makes relative to itself (`%import .lib.X` in `a/b.lark`) reads the
    /// file beside it (`a/lib.lark`), again from the first directory that
    /// has it; one the grammar text itself makes reads a top-level file.
    /// Together the directories act as one layered directory, so unlike
    /// Lark a relative import is not tied to the directory its importer was
    /// found in. Lark's bundled `common`, `python`, `unicode` and `lark`
    /// grammars are used when no directory has the file.
    pub fn import_path(mut self, dir: impl Into<PathBuf>) -> Self {
        self.handle = OnceLock::new();
        self.import_paths.push(dir.into());
        self
    }

    /// Draw a grammar string of at most `budget` characters outside its
    /// markers and a value for each marker, spliced in: the string, how
    /// many of its characters the grammar drew and how many markers, and
    /// how many characters it has.
    fn splice(
        &self,
        tc: &TestCase,
        compiled: &Compiled,
        budget: usize,
    ) -> (String, usize, usize, usize) {
        let drawn = tc.generate_grammar(&compiled.grammar, budget as u64);
        let mut out = String::with_capacity(drawn.len());
        let mut own = 0;
        let mut markers = 0;
        let mut size = 0;
        let mut rest = drawn.as_str();
        let mut from = 0;
        while let Some(at) = rest[from..]
            .find(|c| compiled.markers.iter().any(|m| m.starts_with(c)))
            .map(|at| from + at)
        {
            let Some(i) = compiled
                .markers
                .iter()
                .position(|marker| rest[at..].starts_with(marker.as_str()))
            else {
                from = at + rest[at..].chars().next().unwrap().len_utf8();
                continue;
            };
            own += rest[..at].chars().count();
            out.push_str(&rest[..at]);
            let value = tc.draw_silent(Terminal(&*self.explicit[i].generator));
            size += value.chars().count();
            markers += 1;
            out.push_str(&value);
            rest = &rest[at + compiled.markers[i].len()..];
            from = 0;
        }
        own += rest.chars().count();
        out.push_str(rest);
        (out, own, markers, size + own)
    }

    fn handle(&self) -> &Compiled {
        self.handle.get_or_init(|| {
            if self.max_size == Some(0) {
                invalid_argument!("max_size must be at least 1");
            }
            if self.max_depth == Some(0) {
                invalid_argument!("max_depth must be at least 1");
            }
            let names: Vec<String> = self.explicit.iter().map(|e| e.name.clone()).collect();
            if let Some(name) = names
                .iter()
                .find(|name| names.iter().filter(|n| n == name).count() > 1)
            {
                invalid_argument!("explicit terminal '{name}' is given more than one generator");
            }
            let alphabet = self.alphabet.as_ref().map(|a| a.build_alphabet_handle());
            ffi::Grammar::new(
                &self.text,
                self.start.as_deref(),
                self.max_size.unwrap_or(DEFAULT_MAX_SIZE) as u64,
                self.max_depth.unwrap_or(0),
                alphabet.as_ref(),
                &names,
                &self.import_paths,
            )
            .and_then(|grammar| {
                let markers = names
                    .iter()
                    .map(|name| grammar.placeholder(name))
                    .collect::<Result<_, _>>()?;
                Ok(Compiled { grammar, markers })
            })
            .unwrap_or_else(|msg| invalid_argument!("{msg}"))
        })
    }
}

impl Generator<String> for LarkGenerator {
    fn label(&self) -> u64 {
        self.label
    }

    fn do_draw(&self, tc: &TestCase) -> String {
        let compiled = self.handle();
        if compiled.markers.is_empty() {
            return tc.generate_grammar(&compiled.grammar, 0);
        }
        let max_size = self.max_size.unwrap_or(DEFAULT_MAX_SIZE);
        let mut budget = max_size;
        for _ in 0..ATTEMPTS {
            tc.start_span(ATTEMPT_LABEL);
            let (out, own, markers, size) = self.splice(tc, compiled, budget);
            if size <= max_size {
                tc.stop_span(false);
                return out;
            }
            tc.stop_span(true);
            budget = ((max_size as u128 * (own + markers) as u128 / size as u128) as usize)
                .min(budget.saturating_sub(1))
                .max(1);
        }
        tc.reject()
    }
}

impl PrintableGenerator<String> for LarkGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

/// Generate strings matching a grammar written in the syntax of the
/// [Lark](https://lark-parser.readthedocs.io/en/stable/grammar.html) parsing
/// library, starting from its `start` rule.
///
/// The whole grammar language is supported: rules, terminals with string
/// and regex patterns and priorities, `%import` (including Lark's bundled
/// `common`, `python`, `unicode` and `lark` grammars), `%ignore`,
/// `%declare`, `%override`, `%extend` and templates. Generated strings are
/// ones Lark's Earley parser accepts for the grammar, with `%ignore`d
/// terminals such as whitespace and comments inserted wherever a lexer
/// would skip them; a string Lark's basic lexer would read differently is
/// avoided where the grammar allows it. A draw that cannot produce such
/// a string after several attempts rejects the test case, as a failed
/// [`assume`](crate::TestCase::assume) does, so a grammar that often
/// needs that fails the filter health check.
///
/// Strings range up to 8192 characters by default, biased toward the
/// larger end; use [`max_size`](LarkGenerator::max_size) and
/// [`max_depth`](LarkGenerator::max_depth) to keep them smaller.
///
/// ```no_run
/// use hegel::generators as gs;
///
/// const GRAMMAR: &str = r#"
///     ?start: sum
///     ?sum: product | sum "+" product -> add
///     ?product: atom | product "*" atom -> mul
///     ?atom: NUMBER | "(" sum ")"
///     %import common.NUMBER
///     %import common.WS
///     %ignore WS
/// "#;
///
/// #[hegel::test]
/// fn parses_every_expression(tc: hegel::TestCase) {
///     let expression = tc.draw(gs::from_lark(GRAMMAR));
///     assert!(my_parser::parse(&expression).is_ok());
/// }
/// # mod my_parser { pub fn parse(_: &str) -> Result<(), ()> { Ok(()) } }
/// ```
///
/// See [`LarkGenerator`] for builder methods.
pub fn from_lark(grammar: &str) -> LarkGenerator {
    let mut generator = LarkGenerator {
        text: grammar.to_string(),
        start: None,
        explicit: Vec::new(),
        alphabet: None,
        max_size: None,
        max_depth: None,
        import_paths: Vec::new(),
        label: 0,
        handle: OnceLock::new(),
    };
    generator.relabel();
    generator
}
