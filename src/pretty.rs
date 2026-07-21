//! Pretty-printing of generated values.
//!
//! [`PrettyPrinter`] wraps libhegel's layout engine (an Oppen-style
//! pretty-printer ported from Hypothesis's `hypothesis.vendor.pretty`).
//! Output is built from three primitives: [`PrettyPrinter::text`] emits
//! literal text, [`PrettyPrinter::breakable`] marks a point that renders as
//! a separator when the enclosing group fits on one line and as a newline
//! plus indentation when it does not, and [`PrettyPrinter::begin_group`] /
//! [`PrettyPrinter::end_group`] delimit the groups those decisions are made
//! over. A group either fits — every breakable renders as its separator —
//! or breaks as a whole, outermost groups first.
//!
//! [`PrettyPrintable`] is the protocol a value uses to describe its own
//! representation, in Rust-expression syntax wherever possible. It is
//! implemented for the standard types the generator library produces,
//! derivable for user types with `#[derive(PrettyPrintable)]`, and
//! available for any `Debug` type — without writing an implementation —
//! through [`pretty_print_as_debug!`](crate::pretty_print_as_debug).

use crate::ffi::PrinterHandle;

/// A pretty-printer that lays out text within a maximum line width.
///
/// See the [module docs](self) for the printing model. Rejections of the
/// layout protocol (an [`end_group`](PrettyPrinter::end_group) with no open
/// group) panic, since they indicate a bug in the calling printing code.
///
/// # Example
///
/// ```
/// use hegel::PrettyPrinter;
///
/// let mut p = PrettyPrinter::new(10);
/// p.begin_group(1, "[");
/// p.text("first");
/// p.text(",");
/// p.breakable(" ");
/// p.text("second");
/// p.end_group(1, "]");
/// assert_eq!(p.value(), "[first,\n second]");
/// ```
#[derive(Debug)]
pub struct PrettyPrinter {
    /// `None` is the no-op printer: every emitting method returns without
    /// doing anything, so one drawing body can serve both the silent and the
    /// printing draw paths.
    handle: Option<PrinterHandle>,
}

impl PrettyPrinter {
    /// Create a printer that keeps lines within `max_width` characters where
    /// the group structure allows it.
    pub fn new(max_width: usize) -> Self {
        PrettyPrinter {
            handle: Some(PrinterHandle::new(max_width as u64)),
        }
    }

    /// Create a printer that discards everything printed to it.
    ///
    /// This is how a [`PrintableGenerator`](crate::PrintableGenerator) with
    /// one shared drawing body implements its silent path:
    /// [`Generator::do_draw`](crate::Generator::do_draw) simply calls
    /// `self.do_draw_and_print(tc, &mut PrettyPrinter::noop())`. The
    /// contract that both paths consume identical choices then holds by
    /// construction. Guard any expensive formatting with
    /// [`should_print`](PrettyPrinter::should_print) so the silent path
    /// stays cheap.
    pub fn noop() -> Self {
        PrettyPrinter { handle: None }
    }

    /// Whether printing to this printer produces output: `false` for the
    /// discarding printer returned by [`noop`](PrettyPrinter::noop). Use it
    /// to skip work — formatting a value, say — whose only purpose is to be
    /// printed.
    pub fn should_print(&self) -> bool {
        self.handle.is_some()
    }

    /// Wrap an existing engine printer handle (e.g. a test case's shared
    /// document).
    pub(crate) fn from_handle(handle: PrinterHandle) -> Self {
        PrettyPrinter {
            handle: Some(handle),
        }
    }

    /// Emit literal, unbreakable text.
    ///
    /// Newlines in `s` are honored as unconditional line breaks (equivalent
    /// to [`hard_break`](PrettyPrinter::hard_break), so the new line starts
    /// at the current indentation).
    pub fn text(&mut self, s: &str) {
        let Some(handle) = &self.handle else { return };
        let mut first = true;
        for segment in s.split('\n') {
            if !first {
                handle.hard_break().unwrap();
            }
            first = false;
            if !segment.is_empty() {
                handle.text(segment).unwrap();
            }
        }
    }

    /// Emit a potential break point: renders as `sep` if the enclosing group
    /// fits on the current line, and as a newline plus the current
    /// indentation if the group breaks.
    pub fn breakable(&mut self, sep: &str) {
        let Some(handle) = &self.handle else { return };
        handle.breakable(sep).unwrap();
    }

    /// Emit an unconditional newline followed by the current indentation.
    pub fn hard_break(&mut self) {
        let Some(handle) = &self.handle else { return };
        handle.hard_break().unwrap();
    }

    /// Open a group: emit `open`, then increase the indentation applied by
    /// subsequent break points by `indent` (conventionally the width of
    /// `open`, so continuation lines align just inside the delimiter).
    pub fn begin_group(&mut self, indent: usize, open: &str) {
        let Some(handle) = &self.handle else { return };
        handle.begin_group(indent as u64, open).unwrap();
    }

    /// Close the innermost group: decrease the indentation by `dedent`, then
    /// emit `close`. Panics if no group is open.
    pub fn end_group(&mut self, dedent: usize, close: &str) {
        let Some(handle) = &self.handle else { return };
        handle.end_group(dedent as u64, close).unwrap();
    }

    /// Adjust the indentation applied by subsequent break points by `delta`.
    pub fn shift_indent(&mut self, delta: isize) {
        let Some(handle) = &self.handle else { return };
        handle.shift_indent(delta as i64).unwrap();
    }

    /// Attach a comment to the line currently being written: `text` is
    /// rendered as `  // text` at the end of that line, every group open at
    /// this position is forced to break — nothing else may share a line with
    /// a comment — and the comment is excluded from line-width accounting. A
    /// group forced to break by a comment also breaks before its closing
    /// delimiter, so the delimiter is not caught up in a comment on the
    /// group's last element.
    ///
    /// This is how explain-mode annotations (`// or any other generated
    /// value`) attach to the parts of a reported value they describe. `text`
    /// must not contain newlines; a comment is a single-line construct.
    pub fn comment(&mut self, text: &str) {
        let Some(handle) = &self.handle else { return };
        handle.comment(&format!("  // {text}")).unwrap();
    }

    /// Splice in any outstanding deferred content, flush pending break
    /// points, and return everything printed so far. The discarding printer
    /// returned by [`noop`](PrettyPrinter::noop) always yields the empty
    /// string.
    pub fn value(&mut self) -> String {
        let Some(handle) = &self.handle else {
            return String::new();
        };
        let _ = handle.resolve();
        handle.value().unwrap()
    }

    /// Open a deferred hole at the current position and return a handle to
    /// fill it in later.
    ///
    /// Whatever is printed through the returned [`DeferredPrinter`] — at any
    /// later point, e.g. while a test body runs — appears at the hole's
    /// position when [`value`](PrettyPrinter::value) renders the document,
    /// with line-breaking behaving as if it had been printed inline. This is
    /// how a generator whose value's representation is only known during
    /// test execution (a Hegel-controlled random number generator, say)
    /// prints: it reserves a hole at draw time and records into it as the
    /// value is used.
    pub fn deferred(&mut self) -> DeferredPrinter {
        DeferredPrinter {
            handle: self
                .handle
                .as_ref()
                .map(|handle| handle.deferred().unwrap()),
        }
    }

    /// Open a speculative region: output printed through the returned
    /// [`Speculation`] is held back until [`Speculation::commit`] emits it or
    /// [`Speculation::abort`] discards it. Dropping the `Speculation` without
    /// committing (e.g. on unwind) aborts it.
    ///
    /// This is how draw-time printing survives rejection: a combinator that
    /// may retract a draw — a filter retry, a rejected collection element —
    /// prints each attempt inside a speculative region and only commits the
    /// accepted one.
    pub fn speculate(&mut self) -> Speculation<'_> {
        if let Some(handle) = &self.handle {
            handle.begin_speculative().unwrap();
        }
        Speculation {
            printer: self,
            resolved: false,
        }
    }
}

/// A handle onto a deferred hole in a [`PrettyPrinter`] document; see
/// [`PrettyPrinter::deferred`].
///
/// Once the document renders (or the speculative region the hole was opened
/// inside is aborted), the slot is dead and every method becomes a silent
/// no-op — a value that outlives its test case can keep trying to record
/// without consequence.
#[derive(Debug)]
pub struct DeferredPrinter {
    /// `None` when the slot was opened on a no-op printer: writes discard.
    handle: Option<crate::ffi::PrinterHandle>,
}

impl DeferredPrinter {
    /// Emit literal text into the slot. Newlines are honored as line breaks.
    pub fn text(&mut self, s: &str) {
        let Some(handle) = &self.handle else { return };
        let mut first = true;
        for segment in s.split('\n') {
            if !first {
                let _ = handle.hard_break();
            }
            first = false;
            if !segment.is_empty() {
                let _ = handle.text(segment);
            }
        }
    }

    /// Emit a potential break point into the slot, rendering as `sep` when
    /// the enclosing group fits on one line.
    pub fn breakable(&mut self, sep: &str) {
        let Some(handle) = &self.handle else { return };
        let _ = handle.breakable(sep);
    }
}

/// An open speculative region on a [`PrettyPrinter`]; see
/// [`PrettyPrinter::speculate`].
#[derive(Debug)]
pub struct Speculation<'a> {
    printer: &'a mut PrettyPrinter,
    resolved: bool,
}

impl Speculation<'_> {
    /// The printer to print the speculative output through.
    pub fn printer(&mut self) -> &mut PrettyPrinter {
        self.printer
    }

    /// Close the region, keeping its output.
    pub fn commit(mut self) {
        self.resolved = true;
        if let Some(handle) = &self.printer.handle {
            handle.commit_speculative().unwrap();
        }
    }

    /// Close the region, discarding its output.
    pub fn abort(mut self) {
        self.resolved = true;
        if let Some(handle) = &self.printer.handle {
            handle.abort_speculative().unwrap();
        }
    }
}

/// Dropping an uncommitted speculation — most importantly during an unwind
/// out of a speculative draw, such as a budget-exhausted `StopTest` or a
/// failed assumption mid-attempt — discards its output, so a partial attempt
/// never corrupts the document. The result is deliberately ignored: this can
/// run during a panic, where a second panic would abort the process.
impl Drop for Speculation<'_> {
    fn drop(&mut self) {
        if !self.resolved {
            if let Some(handle) = &self.printer.handle {
                let _ = handle.abort_speculative();
            }
        }
    }
}

/// Print a `{:?}` representation through the layout machinery.
///
/// The output of a derived `Debug` implementation follows a small grammar —
/// `Name { field: value, … }`, `Name(…)`, `(…)`, `[…]`, `{key: value, …}`,
/// string and character literals, atoms — and this function re-emits it
/// through the printer's group and breakable primitives, so a large value
/// wraps exactly like one printed by `#[derive(PrettyPrintable)]`. Anything
/// that doesn't parse as that grammar (a hand-written `Debug` can produce
/// arbitrary text) is emitted verbatim, with embedded newlines honored as
/// hard breaks.
///
/// This is the engine behind [`pretty_print_as_debug!`](crate::pretty_print_as_debug)
/// and [`print_as_debug`](crate::Generator::print_as_debug); it is exposed
/// for hand-written [`PrettyPrintable`] implementations that want to embed a
/// `Debug` representation in a larger layout.
pub fn print_debug_repr(repr: &str, printer: &mut PrettyPrinter) {
    match DebugRepr::parse(repr) {
        Some(nodes) => emit_debug_nodes(&nodes, printer),
        None => printer.text(repr),
    }
}

/// One parsed piece of a `Debug` representation: literal text, or a
/// delimited group laid out with a breakable point after each comma.
enum DebugNode {
    Leaf(String),
    Group {
        /// The atom glued to the open delimiter (`Some` in `Some(5)`, `Name`
        /// in `Name { … }`); empty for bare tuples, lists, and map braces.
        prefix: String,
        delimiter: char,
        /// Brace group in derived struct style (`Name { … }`, spaces inside
        /// the braces) as opposed to map style (`{… }`).
        named: bool,
        items: Vec<Vec<DebugNode>>,
    },
}

/// Recursive-descent parser over the derived-`Debug` grammar. Any input
/// outside the grammar makes a parsing method return `None`, and the whole
/// representation falls back to verbatim text.
struct DebugRepr {
    chars: Vec<char>,
    pos: usize,
}

impl DebugRepr {
    fn parse(repr: &str) -> Option<Vec<DebugNode>> {
        if repr.contains('\n') {
            return None;
        }
        let mut parser = DebugRepr {
            chars: repr.chars().collect(),
            pos: 0,
        };
        let nodes = parser.parse_item()?;
        if parser.pos != parser.chars.len() {
            return None;
        }
        Some(nodes)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    /// Parse one comma-separated item — literal runs and nested groups —
    /// stopping (without consuming) at a `", "`, a close delimiter, or the
    /// end of the input.
    fn parse_item(&mut self) -> Option<Vec<DebugNode>> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        loop {
            match self.peek() {
                None | Some(']' | ')' | '}') => break,
                Some(',') if self.peek_next() == Some(' ') => break,
                Some(' ') if self.peek_next() == Some('}') => break,
                Some('"' | '\'') => {
                    flush_text(&mut text, &mut nodes);
                    nodes.push(DebugNode::Leaf(self.lex_quoted()?));
                }
                Some(delimiter @ ('[' | '(' | '{')) => {
                    let prefix = take_group_prefix(&mut text, delimiter);
                    flush_text(&mut text, &mut nodes);
                    nodes.push(self.parse_group(prefix)?);
                }
                Some(c) => {
                    text.push(c);
                    self.bump();
                }
            }
        }
        flush_text(&mut text, &mut nodes);
        Some(nodes)
    }

    /// Parse a delimited group whose open delimiter is the current char.
    fn parse_group(&mut self, prefix: String) -> Option<DebugNode> {
        let delimiter = self.bump()?;
        let close = match delimiter {
            '[' => ']',
            '(' => ')',
            _ => '}',
        };
        let named = delimiter == '{' && !prefix.is_empty() && self.peek() == Some(' ');
        if named {
            self.bump();
        }
        let mut items = Vec::new();
        if !named && self.peek() == Some(close) {
            self.bump();
        } else {
            loop {
                items.push(self.parse_item()?);
                match self.peek() {
                    Some(',') if self.peek_next() == Some(' ') => {
                        self.bump();
                        self.bump();
                    }
                    Some(' ') if named && self.peek_next() == Some(close) => {
                        self.bump();
                        self.bump();
                        break;
                    }
                    Some(c) if !named && c == close => {
                        self.bump();
                        break;
                    }
                    _ => return None,
                }
            }
        }
        Some(DebugNode::Group {
            prefix,
            delimiter,
            named,
            items,
        })
    }

    /// Lex a string or character literal, including its quotes. A backslash
    /// escapes the following character, which is all the lexer needs: no
    /// escape sequence contains an unescaped closing quote.
    fn lex_quoted(&mut self) -> Option<String> {
        let quote = self.bump()?;
        let mut lit = String::new();
        lit.push(quote);
        loop {
            let c = self.bump()?;
            lit.push(c);
            if c == '\\' {
                lit.push(self.bump()?);
            } else if c == quote {
                return Some(lit);
            }
        }
    }
}

/// Move accumulated literal text into a leaf node.
fn flush_text(text: &mut String, nodes: &mut Vec<DebugNode>) {
    if !text.is_empty() {
        nodes.push(DebugNode::Leaf(std::mem::take(text)));
    }
}

/// Split the atom glued to an open delimiter off the accumulated text:
/// `Some` from `Some(`, and `Name` (dropping the joining space) from
/// `Name {`. Brace groups only take a prefix across that space — a brace
/// directly following text is not the derived-struct shape.
fn take_group_prefix(text: &mut String, delimiter: char) -> String {
    if delimiter == '{' {
        let Some(without_space) = text.strip_suffix(' ') else {
            return String::new();
        };
        let start = without_space
            .rfind(' ')
            .map(|index| index + 1)
            .unwrap_or(0);
        let prefix = without_space[start..].to_string();
        if prefix.is_empty() {
            return String::new();
        }
        text.truncate(text.len() - prefix.len() - 1);
        prefix
    } else {
        let start = text.rfind(' ').map(|index| index + 1).unwrap_or(0);
        let prefix = text[start..].to_string();
        text.truncate(start);
        prefix
    }
}

/// Emit parsed nodes, matching the layout `#[derive(PrettyPrintable)]`
/// produces for the same shapes.
fn emit_debug_nodes(nodes: &[DebugNode], printer: &mut PrettyPrinter) {
    for node in nodes {
        match node {
            DebugNode::Leaf(text) => printer.text(text),
            DebugNode::Group {
                prefix,
                delimiter,
                named,
                items,
            } => {
                let (open, close, indent) = match (delimiter, named) {
                    ('{', true) => (format!("{prefix} {{"), " }", 4),
                    ('{', false) if prefix.is_empty() => ("{".to_string(), "}", 1),
                    ('{', false) => (format!("{prefix} {{"), "}", 1),
                    ('[', _) => (format!("{prefix}["), "]", 1),
                    _ => (format!("{prefix}("), ")", 1),
                };
                printer.begin_group(indent, &open);
                if *named {
                    printer.breakable(" ");
                }
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        printer.text(",");
                        printer.breakable(" ");
                    }
                    emit_debug_nodes(item, printer);
                }
                printer.end_group(indent, close);
            }
        }
    }
}

/// A value that can describe its own printed representation.
///
/// Implementations should print the value in Rust-expression syntax wherever
/// possible, so a reported failing example can be pasted back into code, and
/// should express any internal structure through the printer's group and
/// breakable primitives so large values wrap readably.
///
/// Provided for the standard types the generator library produces. For user
/// types, either `#[derive(PrettyPrintable)]` or — to reuse an existing
/// `Debug` representation without writing anything —
/// [`pretty_print_as_debug!`](crate::pretty_print_as_debug).
pub trait PrettyPrintable {
    /// Print this value's representation to `printer`.
    fn pretty_print(&self, printer: &mut PrettyPrinter);
}

/// Implement [`PrettyPrintable`] for one or more local `Debug` types by
/// printing their `{:?}` representation through
/// [`print_debug_repr`](crate::pretty::print_debug_repr), so derived-`Debug`
/// output wraps like a native implementation.
///
/// This is for **your own types** whose `Debug` output is already the
/// representation you want: the orphan rule means it cannot implement a
/// hegel trait for a type from another crate (including the standard
/// library). To print a foreign type by its `Debug` representation, make
/// the *generator* printable instead with
/// [`print_as_debug`](crate::Generator::print_as_debug).
///
/// ```
/// use hegel::{PrettyPrintable, PrettyPrinter};
///
/// #[derive(Debug)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
/// hegel::pretty_print_as_debug!(Point);
///
/// let mut p = PrettyPrinter::new(79);
/// Point { x: 1, y: 2 }.pretty_print(&mut p);
/// assert_eq!(p.value(), "Point { x: 1, y: 2 }");
/// ```
#[macro_export]
macro_rules! pretty_print_as_debug {
    ($($t:ty),+ $(,)?) => {$(
        impl $crate::PrettyPrintable for $t {
            fn pretty_print(&self, printer: &mut $crate::PrettyPrinter) {
                $crate::pretty::print_debug_repr(&format!("{:?}", self), printer);
            }
        }
    )+};
}

macro_rules! pretty_via_display {
    ($($t:ty),+) => {$(
        impl PrettyPrintable for $t {
            fn pretty_print(&self, printer: &mut PrettyPrinter) {
                printer.text(&format!("{}", self));
            }
        }
    )+};
}

pretty_via_display!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, bool
);

macro_rules! pretty_via_debug {
    ($($t:ty),+) => {$(
        impl PrettyPrintable for $t {
            fn pretty_print(&self, printer: &mut PrettyPrinter) {
                printer.text(&format!("{:?}", self));
            }
        }
    )+};
}

pretty_via_debug!(
    char,
    str,
    std::time::Duration,
    std::net::IpAddr,
    std::net::Ipv4Addr,
    std::net::Ipv6Addr
);

impl PrettyPrintable for String {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        self.as_str().pretty_print(printer);
    }
}

macro_rules! pretty_float {
    ($t:ty, $name:literal) => {
        impl PrettyPrintable for $t {
            fn pretty_print(&self, printer: &mut PrettyPrinter) {
                if self.is_nan() {
                    if self.to_bits() == <$t>::NAN.to_bits() {
                        printer.text(concat!($name, "::NAN"));
                    } else {
                        printer.text(&format!(
                            concat!($name, "::from_bits(0x{:x})"),
                            self.to_bits()
                        ));
                    }
                } else if *self == <$t>::INFINITY {
                    printer.text(concat!($name, "::INFINITY"));
                } else if *self == <$t>::NEG_INFINITY {
                    printer.text(concat!($name, "::NEG_INFINITY"));
                } else {
                    printer.text(&format!("{:?}", self));
                }
            }
        }
    };
}

pretty_float!(f32, "f32");
pretty_float!(f64, "f64");

macro_rules! pretty_delegating {
    ($($t:ty),+) => {$(
        impl<T: PrettyPrintable + ?Sized> PrettyPrintable for $t {
            fn pretty_print(&self, printer: &mut PrettyPrinter) {
                (**self).pretty_print(printer);
            }
        }
    )+};
}

pretty_delegating!(&T, &mut T, Box<T>, std::rc::Rc<T>, std::sync::Arc<T>);

/// Print `items` as a delimited, comma-separated sequence: inline when it
/// fits, one element per line (aligned just inside `open`) when it does not.
fn pretty_seq<'a, T: PrettyPrintable + ?Sized + 'a>(
    printer: &mut PrettyPrinter,
    open: &str,
    close: &str,
    items: impl Iterator<Item = &'a T>,
) {
    printer.begin_group(open.chars().count(), open);
    let mut index = 0usize;
    for item in items {
        if index > 0 {
            printer.text(",");
            printer.breakable(" ");
        }
        index += 1;
        item.pretty_print(printer);
    }
    let _ = index;
    printer.end_group(close.chars().count(), close);
}

impl<T: PrettyPrintable> PrettyPrintable for [T] {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        pretty_seq(printer, "[", "]", self.iter());
    }
}

impl<T: PrettyPrintable> PrettyPrintable for Vec<T> {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        self.as_slice().pretty_print(printer);
    }
}

impl<T: PrettyPrintable, const N: usize> PrettyPrintable for [T; N] {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        self.as_slice().pretty_print(printer);
    }
}

impl<T: PrettyPrintable, S> PrettyPrintable for std::collections::HashSet<T, S> {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        pretty_seq(printer, "{", "}", self.iter());
    }
}

impl<T: PrettyPrintable> PrettyPrintable for std::collections::BTreeSet<T> {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        pretty_seq(printer, "{", "}", self.iter());
    }
}

/// Print `entries` as a `{key: value, …}` map: inline when it fits, one
/// entry per line when it does not.
fn pretty_map<'a, K: PrettyPrintable + 'a, V: PrettyPrintable + 'a>(
    printer: &mut PrettyPrinter,
    entries: impl Iterator<Item = (&'a K, &'a V)>,
) {
    printer.begin_group(1, "{");
    let mut index = 0usize;
    for (key, value) in entries {
        if index > 0 {
            printer.text(",");
            printer.breakable(" ");
        }
        index += 1;
        key.pretty_print(printer);
        printer.text(": ");
        value.pretty_print(printer);
    }
    let _ = index;
    printer.end_group(1, "}");
}

impl<K: PrettyPrintable, V: PrettyPrintable, S> PrettyPrintable
    for std::collections::HashMap<K, V, S>
{
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        pretty_map(printer, self.iter());
    }
}

impl<K: PrettyPrintable, V: PrettyPrintable> PrettyPrintable for std::collections::BTreeMap<K, V> {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        pretty_map(printer, self.iter());
    }
}

impl<T: PrettyPrintable> PrettyPrintable for Option<T> {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        match self {
            None => printer.text("None"),
            Some(value) => {
                printer.begin_group(5, "Some(");
                value.pretty_print(printer);
                printer.end_group(5, ")");
            }
        }
    }
}

impl<T: PrettyPrintable, E: PrettyPrintable> PrettyPrintable for Result<T, E> {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        match self {
            Ok(value) => {
                printer.begin_group(3, "Ok(");
                value.pretty_print(printer);
                printer.end_group(3, ")");
            }
            Err(error) => {
                printer.begin_group(4, "Err(");
                error.pretty_print(printer);
                printer.end_group(4, ")");
            }
        }
    }
}

impl PrettyPrintable for () {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        printer.text("()");
    }
}

impl<A: PrettyPrintable> PrettyPrintable for (A,) {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        printer.begin_group(1, "(");
        self.0.pretty_print(printer);
        printer.end_group(1, ",)");
    }
}

macro_rules! pretty_tuple {
    ($(($($name:ident),+)),+ $(,)?) => {$(
        #[allow(non_snake_case)]
        impl<$($name: PrettyPrintable),+> PrettyPrintable for ($($name,)+) {
            fn pretty_print(&self, printer: &mut PrettyPrinter) {
                let ($($name,)+) = self;
                printer.begin_group(1, "(");
                let mut index = 0usize;
                $(
                    if index > 0 {
                        printer.text(",");
                        printer.breakable(" ");
                    }
                    index += 1;
                    $name.pretty_print(printer);
                )+
                let _ = index;
                printer.end_group(1, ")");
            }
        }
    )+};
}

pretty_tuple!(
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F),
    (A, B, C, D, E, F, G),
    (A, B, C, D, E, F, G, H),
    (A, B, C, D, E, F, G, H, I),
    (A, B, C, D, E, F, G, H, I, J),
    (A, B, C, D, E, F, G, H, I, J, K),
    (A, B, C, D, E, F, G, H, I, J, K, L),
);
