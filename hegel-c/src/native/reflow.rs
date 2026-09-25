//! Re-laying-out a one-line debug representation through the printer.
//!
//! Every language has a default way of showing a value for debugging —
//! Rust's `{:?}`, Go's `%#v`, Python's `repr`, JavaScript's `util.inspect`,
//! Java records' `toString`, OCaml's toplevel printer — and every one of
//! them produces a single line, however large the value. The reflower
//! takes that line and re-emits it through [`Printer`]'s group and
//! breakable primitives, so the value wraps like one that had been printed
//! structurally, without any client-side knowledge of the value's type.
//!
//! The grammar is deliberately language-agnostic: a representation is a
//! sequence of *items*, where an item is a run of atoms, quoted literals
//! (`"…"` or `'…'`, with backslash escapes) and bracketed *groups*. A group
//! is `(`, `[` or `{` through its matching close, containing items
//! separated by `, ` or `; `. Any atom glued to the open delimiter (the
//! `Some` in `Some(5)`, the `main.Point` in `main.Point{X:1}`, the
//! `Point ` in `Point { x: 1 }`) is the group's prefix and stays on its
//! line. A group whose open delimiter is followed by a space is *padded*
//! (`Point { x: 1 }`, `{ a: 1, b: 2 }`) and expects a space before its
//! close. Delimiter text is preserved verbatim, so the flat rendering of
//! any parsed input is the input itself.
//!
//! A group lays out inline when it fits and one item per line when it does
//! not: a padded group in block style (items indented four columns past the
//! group's own line, the close on the last item's line), any other group
//! aligned just inside its open delimiter. A representation that does not
//! parse — a hand-written formatter can produce anything — is emitted
//! verbatim, as is one nested deeper than [`MAX_DEPTH`], with embedded
//! newlines honored as hard breaks.

use alloc::string::String;
use alloc::vec::Vec;

use super::printer::{Printer, PrinterError, Target};

/// Options for [`reflow`]. There are none yet; the type exists so that the
/// C ABI's `hegel_reflow_options_t` handle — and every binding's signature
/// for the reflow call — is stable when options arrive.
#[derive(Debug, Clone, Default)]
pub struct ReflowOptions {}

/// How deeply groups may nest before the reflower gives up and emits the
/// representation verbatim. Parsing, emission and the parsed tree's
/// destructor all recurse per nesting level, so an unbounded representation
/// would overflow the stack during failure reporting.
pub const MAX_DEPTH: usize = 64;

/// Indentation of a broken padded group's items past the group's own line.
const BLOCK_INDENT: usize = 4;

/// Re-emit `repr` through `printer` on `target`; see the module docs.
pub fn reflow(
    printer: &mut Printer,
    target: Target,
    repr: &str,
    _options: &ReflowOptions,
) -> Result<(), PrinterError> {
    match Parser::parse(repr) {
        Some(nodes) => emit_nodes(printer, target, &nodes),
        None => emit_verbatim(printer, target, repr),
    }
}

fn emit_verbatim(printer: &mut Printer, target: Target, repr: &str) -> Result<(), PrinterError> {
    for (index, line) in repr.split('\n').enumerate() {
        if index > 0 {
            printer.hard_break(target)?;
        }
        if !line.is_empty() {
            printer.text(target, line)?;
        }
    }
    Ok(())
}

enum Node {
    Leaf(String),
    Group(Group),
}

struct Group {
    /// The text glued to the open delimiter, including a joining space for
    /// the `Name {` shape.
    prefix: String,
    open: char,
    close: char,
    padded: bool,
    items: Vec<Vec<Node>>,
    /// The separator character before each item after the first.
    separators: Vec<char>,
}

impl Group {
    fn open_text(&self) -> String {
        let mut text = self.prefix.clone();
        text.push(self.open);
        text
    }

    fn close_text(&self) -> String {
        let mut text = String::new();
        if self.padded {
            text.push(' ');
        }
        text.push(self.close);
        text
    }

    /// Whether this group contains no nested groups, so that it can be
    /// folded back into a line of text.
    fn is_shallow(&self) -> bool {
        self.items
            .iter()
            .all(|item| item.iter().all(|node| matches!(node, Node::Leaf(_))))
    }

    /// The group as it was written; only called on shallow groups.
    fn flat_text(&self) -> String {
        let mut text = self.open_text();
        if self.padded && !self.items.is_empty() {
            text.push(' ');
        }
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                text.push(self.separators[index - 1]);
                text.push(' ');
            }
            for node in item {
                if let Node::Leaf(leaf) = node {
                    text.push_str(leaf);
                }
            }
        }
        if self.padded && !self.items.is_empty() {
            text.push(' ');
        }
        text.push(self.close);
        text
    }
}

fn closing(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn parse(repr: &str) -> Option<Vec<Node>> {
        if repr.contains('\n') {
            return None;
        }
        let mut parser = Parser {
            chars: repr.chars().collect(),
            pos: 0,
            depth: 0,
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

    fn at_separator(&self) -> bool {
        matches!(self.peek(), Some(',' | ';')) && self.peek_next() == Some(' ')
    }

    /// Parse one item — literal runs and nested groups — stopping without
    /// consuming at a separator, a close delimiter (optionally preceded by
    /// the space of a padded group), or the end of the input.
    fn parse_item(&mut self) -> Option<Vec<Node>> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        loop {
            match self.peek() {
                None | Some(']' | ')' | '}') => break,
                Some(',' | ';') if self.at_separator() => break,
                Some(' ') if matches!(self.peek_next(), Some(']' | ')' | '}')) => break,
                Some('"' | '\'') => {
                    flush_text(&mut text, &mut nodes);
                    nodes.push(Node::Leaf(self.lex_quoted()?));
                }
                Some(open @ ('[' | '(' | '{')) => {
                    let prefix = take_prefix(&mut text, &mut nodes);
                    flush_text(&mut text, &mut nodes);
                    nodes.push(Node::Group(self.parse_group(prefix, open)?));
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

    /// Parse a group whose open delimiter is the current char.
    fn parse_group(&mut self, prefix: String, open: char) -> Option<Group> {
        if self.depth == MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        self.bump();
        let close = closing(open);
        let padded = self.peek() == Some(' ');
        if padded {
            self.bump();
        }
        let mut items = Vec::new();
        let mut separators = Vec::new();
        if self.peek() == Some(close) && !padded {
            self.bump();
        } else {
            loop {
                items.push(self.parse_item()?);
                match self.peek() {
                    Some(sep) if self.at_separator() => {
                        separators.push(sep);
                        self.bump();
                        self.bump();
                    }
                    Some(' ') if padded && self.peek_next() == Some(close) => {
                        self.bump();
                        self.bump();
                        break;
                    }
                    Some(c) if !padded && c == close => {
                        self.bump();
                        break;
                    }
                    _ => return None,
                }
            }
        }
        self.depth -= 1;
        Some(Group {
            prefix,
            open,
            close,
            padded,
            items,
            separators,
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

fn flush_text(text: &mut String, nodes: &mut Vec<Node>) {
    if !text.is_empty() {
        nodes.push(Node::Leaf(core::mem::take(text)));
    }
}

/// Split the prefix glued to an open delimiter off the item parsed so far:
/// the last space-separated word of the pending text (`Some` from `Some(`,
/// `main.Point` from `main.Point{`), keeping one joining space (`Point `
/// from `Point {`). A word that reaches back past the pending text — Go's
/// `[]int{` or `map[string]int{`, JavaScript's `Map(2) {` — continues
/// through the preceding shallow groups and leaves of the item, so the
/// group's open text is the whole type expression and its items align
/// past it.
fn take_prefix(text: &mut String, nodes: &mut Vec<Node>) -> String {
    let joiner = if text.ends_with(' ') {
        text.pop();
        " "
    } else {
        ""
    };
    let start = text.rfind(' ').map(|index| index + 1);
    let mut prefix = text.split_off(start.unwrap_or(0));
    if start.is_none() {
        while let Some(node) = nodes.pop() {
            match node {
                Node::Leaf(mut leaf) => match leaf.rfind(' ') {
                    Some(index) => {
                        prefix.insert_str(0, &leaf.split_off(index + 1));
                        nodes.push(Node::Leaf(leaf));
                        break;
                    }
                    None => prefix.insert_str(0, &leaf),
                },
                Node::Group(group) if group.is_shallow() => {
                    prefix.insert_str(0, &group.flat_text());
                }
                deep => {
                    nodes.push(deep);
                    break;
                }
            }
        }
    }
    prefix.push_str(joiner);
    prefix
}

fn emit_nodes(printer: &mut Printer, target: Target, nodes: &[Node]) -> Result<(), PrinterError> {
    for node in nodes {
        match node {
            Node::Leaf(text) => printer.text(target, text)?,
            Node::Group(group) => {
                let open = group.open_text();
                let indent = if group.padded {
                    BLOCK_INDENT
                } else {
                    open.chars().count()
                };
                printer.begin_group(target, indent, &open)?;
                if group.padded {
                    printer.breakable(target, " ")?;
                }
                for (index, item) in group.items.iter().enumerate() {
                    if index > 0 {
                        let mut separator = String::new();
                        separator.push(group.separators[index - 1]);
                        printer.text(target, &separator)?;
                        printer.breakable(target, " ")?;
                    }
                    emit_nodes(printer, target, item)?;
                }
                printer.end_group(target, &group.close_text())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/embedded/native/reflow_tests.rs"]
mod tests;
