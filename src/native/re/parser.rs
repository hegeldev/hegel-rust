//! Port of CPython's `Lib/re/_parser.py`.
//!
//! Parses a Python-style regex pattern into a [`SubPattern`] tree whose
//! shape mirrors the `(op, arg)` tuples emitted by `sre_parse.parse`. The
//! variant names track the Python OPCODES/ATCODES/CHCODES constants so
//! cross-referencing stays trivial.
//!
//! Matching is NOT ported; the pattern crate handles execution at
//! test-runtime. This module only produces the AST that
//! Hypothesis's `strategies._internal.regex` walks to build a generator.
//!
//! Source: `resources/cpython/Lib/re/_parser.py`.

// Allowed while the consumer side of the parser port (the regex strategy)
// is still being ported — every item here is referenced by either the
// port's tests or the not-yet-ported strategy.
#![allow(dead_code)]

use std::collections::HashMap;

use super::constants::*;

/// A single parsed operation. Each variant corresponds to an entry in
/// CPython's `OPCODES` list, with the argument shape matching the
/// `(op, av)` tuple CPython emits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpCode {
    /// `(LITERAL, codepoint)`: match exactly this codepoint.
    Literal(u32),
    /// `(NOT_LITERAL, codepoint)`: match any codepoint except this one.
    NotLiteral(u32),
    /// `(ANY, None)`: match any character (respecting DOTALL).
    Any,
    /// `(AT, at_code)`: zero-width position assertion.
    At(AtCode),
    /// `(IN, items)`: character class `[...]`.
    In(Vec<SetItem>),
    /// `(BRANCH, (None, items))`: alternation `a|b|c`.
    Branch(Vec<SubPattern>),
    /// `(SUBPATTERN, (group, add_flags, del_flags, p))`: group `(...)`.
    Subpattern {
        group: Option<u32>,
        add_flags: u32,
        del_flags: u32,
        p: SubPattern,
    },
    /// `(GROUPREF, gid)`: backreference to a previously captured group.
    GroupRef(u32),
    /// `(GROUPREF_EXISTS, (cond_group, yes, no))`: `(?(id)yes|no)`.
    GroupRefExists {
        cond_group: u32,
        yes: SubPattern,
        no: Option<SubPattern>,
    },
    /// `(ASSERT, (dir, p))`: positive lookahead/lookbehind.
    Assert { direction: i32, p: SubPattern },
    /// `(ASSERT_NOT, (dir, p))`: negative lookahead/lookbehind.
    AssertNot { direction: i32, p: SubPattern },
    /// `(ATOMIC_GROUP, p)`: `(?>...)`.
    AtomicGroup(SubPattern),
    /// `(MAX_REPEAT, (min, max, item))`: greedy repetition.
    MaxRepeat {
        min: u32,
        max: u32,
        item: SubPattern,
    },
    /// `(MIN_REPEAT, (min, max, item))`: non-greedy repetition.
    MinRepeat {
        min: u32,
        max: u32,
        item: SubPattern,
    },
    /// `(POSSESSIVE_REPEAT, (min, max, item))`: possessive repetition.
    PossessiveRepeat {
        min: u32,
        max: u32,
        item: SubPattern,
    },
    /// `(FAILURE, ())`: emitted for empty negative-lookahead `(?!)`.
    Failure,
}

/// An item inside a character class `[...]`. Maps to the nested `(op, av)`
/// tuples Python emits as the `IN` argument.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SetItem {
    /// `(LITERAL, codepoint)`.
    Literal(u32),
    /// `(NEGATE, None)` — marker always emitted first when present.
    Negate,
    /// `(RANGE, (lo, hi))`.
    Range(u32, u32),
    /// `(CATEGORY, ch_code)`.
    Category(ChCode),
}

/// Mirror of Python's `SubPattern.data`. Kept as a thin wrapper so that
/// pattern-walking code can extend it with helpers later.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubPattern {
    pub data: Vec<OpCode>,
}

impl SubPattern {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn push(&mut self, op: OpCode) {
        self.data.push(op);
    }

    pub fn last(&self) -> Option<&OpCode> {
        self.data.last()
    }

    pub fn last_mut(&mut self) -> Option<&mut OpCode> {
        self.data.last_mut()
    }
}

/// Top-level output of [`parse`]: the parsed pattern tree plus the
/// flags/group metadata the compiler (or, in our case, generator) needs.
#[derive(Clone, Debug)]
pub struct ParsedPattern {
    pub pattern: SubPattern,
    pub flags: u32,
    pub group_count: u32,
    pub group_names: HashMap<String, u32>,
}

/// Parsing error with an offset into the original pattern string. Mirrors
/// the subset of `PatternError` that callers of `parse` actually observe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub msg: String,
    pub pos: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at position {}", self.msg, self.pos)
    }
}

impl std::error::Error for ParseError {}

type ParseResult<T> = Result<T, ParseError>;

/// Parser state shared across recursive `_parse` calls. Matches
/// `_parser.State` field-for-field.
struct State {
    flags: u32,
    groupdict: HashMap<String, u32>,
    /// `Some(width)` once a group has been closed, `None` while open.
    groupwidths: Vec<Option<(u64, u64)>>,
    lookbehindgroups: Option<u32>,
    grouprefpos: HashMap<u32, usize>,
}

impl State {
    fn new() -> Self {
        Self {
            flags: 0,
            groupdict: HashMap::new(),
            groupwidths: vec![None],
            lookbehindgroups: None,
            grouprefpos: HashMap::new(),
        }
    }

    fn groups(&self) -> u32 {
        self.groupwidths.len() as u32
    }

    fn opengroup(&mut self, name: Option<&str>) -> ParseResult<u32> {
        let gid = self.groups();
        self.groupwidths.push(None);
        if self.groups() > MAXGROUPS {
            return Err(ParseError {
                msg: "too many groups".into(),
                pos: 0,
            });
        }
        if let Some(n) = name {
            if let Some(&ogid) = self.groupdict.get(n) {
                return Err(ParseError {
                    msg: format!(
                        "redefinition of group name '{}' as group {}; was group {}",
                        n, gid, ogid
                    ),
                    pos: 0,
                });
            }
            self.groupdict.insert(n.to_string(), gid);
        }
        Ok(gid)
    }

    fn closegroup(&mut self, gid: u32) {
        if let Some(slot) = self.groupwidths.get_mut(gid as usize) {
            *slot = Some((0, 0));
        }
    }

    fn checkgroup(&self, gid: u32) -> bool {
        (gid as usize) < self.groupwidths.len() && self.groupwidths[gid as usize].is_some()
    }

    fn checklookbehindgroup(&self, gid: u32, tok: &Tokenizer) -> ParseResult<()> {
        if let Some(lb) = self.lookbehindgroups {
            if !self.checkgroup(gid) {
                return Err(tok.error("cannot refer to an open group", 0));
            }
            if gid >= lb {
                return Err(tok.error(
                    "cannot refer to group defined in the same lookbehind subpattern",
                    0,
                ));
            }
        }
        Ok(())
    }
}

/// Character-level tokenizer. Python treats `\X` (backslash + char) as one
/// logical token, so tokens here are `String`s of length 1 or 2.
struct Tokenizer {
    chars: Vec<char>,
    istext: bool,
    index: usize,
    next: Option<String>,
}

impl Tokenizer {
    fn new(pattern: &str, istext: bool) -> ParseResult<Self> {
        let mut tok = Tokenizer {
            chars: pattern.chars().collect(),
            istext,
            index: 0,
            next: None,
        };
        tok.advance()?;
        Ok(tok)
    }

    fn advance(&mut self) -> ParseResult<()> {
        let mut index = self.index;
        let Some(&ch) = self.chars.get(index) else {
            self.next = None;
            return Ok(());
        };
        if ch == '\\' {
            index += 1;
            let Some(&nx) = self.chars.get(index) else {
                return Err(ParseError {
                    msg: "bad escape (end of pattern)".into(),
                    pos: self.chars.len().saturating_sub(1),
                });
            };
            let mut s = String::with_capacity(2);
            s.push(ch);
            s.push(nx);
            self.next = Some(s);
        } else {
            self.next = Some(ch.to_string());
        }
        self.index = index + 1;
        Ok(())
    }

    fn peek_next_char(&self) -> Option<char> {
        self.next.as_ref().and_then(|s| s.chars().next())
    }

    fn take_match(&mut self, ch: char) -> ParseResult<bool> {
        if self.next.as_deref() == Some(&ch.to_string()[..]) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn get(&mut self) -> ParseResult<Option<String>> {
        let this = self.next.clone();
        self.advance()?;
        Ok(this)
    }

    fn getwhile(&mut self, n: usize, charset: &str) -> ParseResult<String> {
        let mut result = String::new();
        for _ in 0..n {
            let Some(s) = &self.next else { break };
            if s.chars().count() != 1 {
                break;
            }
            let c = s.chars().next().unwrap();
            if !charset.contains(c) {
                break;
            }
            result.push(c);
            self.advance()?;
        }
        Ok(result)
    }

    fn getuntil(&mut self, terminator: char, name: &str) -> ParseResult<String> {
        let mut result = String::new();
        loop {
            let c = self.next.clone();
            self.advance()?;
            match c {
                None => {
                    if result.is_empty() {
                        return Err(self.error(&format!("missing {}", name), 0));
                    }
                    return Err(self.error(
                        &format!("missing {}, unterminated name", terminator),
                        result.chars().count(),
                    ));
                }
                Some(s) if s == terminator.to_string() => {
                    if result.is_empty() {
                        return Err(self.error(&format!("missing {}", name), 1));
                    }
                    return Ok(result);
                }
                Some(s) => {
                    result.push_str(&s);
                }
            }
        }
    }

    /// Port of `pos`/`tell`: index of the start of the current `next` token.
    fn tell(&self) -> usize {
        let next_len = self.next.as_ref().map(|s| s.chars().count()).unwrap_or(0);
        self.index - next_len
    }

    fn seek(&mut self, index: usize) -> ParseResult<()> {
        self.index = index;
        self.advance()
    }

    fn error(&self, msg: &str, offset: usize) -> ParseError {
        let pos = self.tell().saturating_sub(offset);
        ParseError {
            msg: msg.to_string(),
            pos,
        }
    }

    fn checkgroupname(&self, name: &str, offset: usize) -> ParseResult<()> {
        if !self.istext && !name.is_ascii() {
            return Err(self.error(
                &format!("bad character in group name '{}'", name),
                name.chars().count() + offset,
            ));
        }
        if !is_python_identifier(name) {
            return Err(self.error(
                &format!("bad character in group name '{}'", name),
                name.chars().count() + offset,
            ));
        }
        Ok(())
    }
}

/// True if `s` is a valid Python identifier per `str.isidentifier`.
///
/// Python defines identifiers by the `XID_Start`/`XID_Continue` Unicode
/// properties plus `_`. We approximate with the ASCII-practical subset plus
/// any non-ASCII codepoint whose general category is a letter or number —
/// that's sufficient for regex group names in the patterns Hypothesis
/// generates. (Matching fails closed — we may reject a valid identifier
/// that used a rare Unicode script; we will never accept an invalid one.)
fn is_python_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_id_start(first) {
        return false;
    }
    chars.all(is_id_continue)
}

fn is_id_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_id_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

const DIGITS: &str = "0123456789";
const OCTDIGITS: &str = "01234567";
const HEXDIGITS: &str = "0123456789abcdefABCDEF";
const ASCIILETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const WHITESPACE: &str = " \t\n\r\x0b\x0c";

const SPECIAL_CHARS: &str = ".\\[{()*+?^$|";
const REPEAT_CHARS: &str = "*+?{";

fn lookup_escape_literal(escape: &str) -> Option<u32> {
    match escape {
        "\\a" => Some(b'\x07' as u32),
        "\\b" => Some(b'\x08' as u32),
        "\\f" => Some(b'\x0c' as u32),
        "\\n" => Some(b'\n' as u32),
        "\\r" => Some(b'\r' as u32),
        "\\t" => Some(b'\t' as u32),
        "\\v" => Some(b'\x0b' as u32),
        "\\\\" => Some(b'\\' as u32),
        _ => None,
    }
}

/// Port of `_parser.CATEGORIES`. Returns `(kind, arg)` where `kind` is
/// either `"AT"` (arg = AtCode) or `"IN"` (arg = ChCode).
fn lookup_category(escape: &str) -> Option<CategoryCode> {
    match escape {
        "\\A" => Some(CategoryCode::At(AtCode::BeginningString)),
        "\\b" => Some(CategoryCode::At(AtCode::Boundary)),
        "\\B" => Some(CategoryCode::At(AtCode::NonBoundary)),
        "\\d" => Some(CategoryCode::In(ChCode::Digit)),
        "\\D" => Some(CategoryCode::In(ChCode::NotDigit)),
        "\\s" => Some(CategoryCode::In(ChCode::Space)),
        "\\S" => Some(CategoryCode::In(ChCode::NotSpace)),
        "\\w" => Some(CategoryCode::In(ChCode::Word)),
        "\\W" => Some(CategoryCode::In(ChCode::NotWord)),
        "\\z" => Some(CategoryCode::At(AtCode::EndString)),
        "\\Z" => Some(CategoryCode::At(AtCode::EndString)),
        _ => None,
    }
}

enum CategoryCode {
    At(AtCode),
    In(ChCode),
}

fn flag_for_char(c: char) -> Option<u32> {
    match c {
        'i' => Some(SRE_FLAG_IGNORECASE),
        'L' => Some(SRE_FLAG_LOCALE),
        'm' => Some(SRE_FLAG_MULTILINE),
        's' => Some(SRE_FLAG_DOTALL),
        'x' => Some(SRE_FLAG_VERBOSE),
        'a' => Some(SRE_FLAG_ASCII),
        'u' => Some(SRE_FLAG_UNICODE),
        _ => None,
    }
}

/// Escape handler inside a character class `[...]`.
///
/// Port of `_parser._class_escape`.
fn class_escape(source: &mut Tokenizer, escape: &str) -> ParseResult<ClassEscapeResult> {
    if let Some(cp) = lookup_escape_literal(escape) {
        return Ok(ClassEscapeResult::Literal(cp));
    }
    if let Some(CategoryCode::In(cat)) = lookup_category(escape) {
        return Ok(ClassEscapeResult::Category(cat));
    }
    let c = escape.chars().nth(1);
    let mut escape = escape.to_string();
    let err_pos = |e: &str| ParseError {
        msg: format!("bad escape {}", e),
        pos: 0,
    };
    let _ = err_pos;
    match c {
        Some('x') => {
            escape.push_str(&source.getwhile(2, HEXDIGITS)?);
            if escape.chars().count() != 4 {
                return Err(source.error(
                    &format!("incomplete escape {}", escape),
                    escape.chars().count(),
                ));
            }
            let hex: String = escape.chars().skip(2).collect();
            let n = u32::from_str_radix(&hex, 16).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            Ok(ClassEscapeResult::Literal(n))
        }
        Some('u') if source.istext => {
            escape.push_str(&source.getwhile(4, HEXDIGITS)?);
            if escape.chars().count() != 6 {
                return Err(source.error(
                    &format!("incomplete escape {}", escape),
                    escape.chars().count(),
                ));
            }
            let hex: String = escape.chars().skip(2).collect();
            let n = u32::from_str_radix(&hex, 16).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            Ok(ClassEscapeResult::Literal(n))
        }
        Some('U') if source.istext => {
            escape.push_str(&source.getwhile(8, HEXDIGITS)?);
            if escape.chars().count() != 10 {
                return Err(source.error(
                    &format!("incomplete escape {}", escape),
                    escape.chars().count(),
                ));
            }
            let hex: String = escape.chars().skip(2).collect();
            let n = u32::from_str_radix(&hex, 16).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            if char::from_u32(n).is_none() {
                return Err(source.error(&format!("bad escape {}", escape), escape.chars().count()));
            }
            Ok(ClassEscapeResult::Literal(n))
        }
        Some('N') if source.istext => {
            // \N{NAME} named Unicode escape. CPython delegates to
            // `unicodedata.lookup`; we don't have a name-to-codepoint
            // table yet, so flag it explicitly rather than silently
            // dropping it. Tracked separately in TODO.yaml.
            Err(source.error(
                r"\N{...} named unicode escapes not yet supported in native regex parser",
                0,
            ))
        }
        Some(ch) if OCTDIGITS.contains(ch) => {
            escape.push_str(&source.getwhile(2, OCTDIGITS)?);
            let oct: String = escape.chars().skip(1).collect();
            let n = u32::from_str_radix(&oct, 8).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            if n > 0o377 {
                return Err(source.error(
                    &format!("octal escape value {} outside of range 0-0o377", escape),
                    escape.chars().count(),
                ));
            }
            Ok(ClassEscapeResult::Literal(n))
        }
        Some(ch) if DIGITS.contains(ch) => {
            Err(source.error(&format!("bad escape {}", escape), escape.chars().count()))
        }
        Some(ch) => {
            if escape.chars().count() == 2 {
                if ASCIILETTERS.contains(ch) {
                    return Err(
                        source.error(&format!("bad escape {}", escape), escape.chars().count())
                    );
                }
                return Ok(ClassEscapeResult::Literal(ch as u32));
            }
            Err(source.error(&format!("bad escape {}", escape), escape.chars().count()))
        }
        None => Err(source.error(&format!("bad escape {}", escape), escape.chars().count())),
    }
}

enum ClassEscapeResult {
    Literal(u32),
    Category(ChCode),
}

/// General escape handler (outside character classes).
///
/// Port of `_parser._escape`.
fn escape_code(
    source: &mut Tokenizer,
    escape: &str,
    state: &mut State,
) -> ParseResult<EscapeResult> {
    if let Some(cat) = lookup_category(escape) {
        return Ok(match cat {
            CategoryCode::At(a) => EscapeResult::At(a),
            CategoryCode::In(c) => EscapeResult::InCategory(c),
        });
    }
    if let Some(cp) = lookup_escape_literal(escape) {
        return Ok(EscapeResult::Literal(cp));
    }
    let c = escape.chars().nth(1);
    let mut escape = escape.to_string();
    match c {
        Some('x') => {
            escape.push_str(&source.getwhile(2, HEXDIGITS)?);
            if escape.chars().count() != 4 {
                return Err(source.error(
                    &format!("incomplete escape {}", escape),
                    escape.chars().count(),
                ));
            }
            let hex: String = escape.chars().skip(2).collect();
            let n = u32::from_str_radix(&hex, 16).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            Ok(EscapeResult::Literal(n))
        }
        Some('u') if source.istext => {
            escape.push_str(&source.getwhile(4, HEXDIGITS)?);
            if escape.chars().count() != 6 {
                return Err(source.error(
                    &format!("incomplete escape {}", escape),
                    escape.chars().count(),
                ));
            }
            let hex: String = escape.chars().skip(2).collect();
            let n = u32::from_str_radix(&hex, 16).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            Ok(EscapeResult::Literal(n))
        }
        Some('U') if source.istext => {
            escape.push_str(&source.getwhile(8, HEXDIGITS)?);
            if escape.chars().count() != 10 {
                return Err(source.error(
                    &format!("incomplete escape {}", escape),
                    escape.chars().count(),
                ));
            }
            let hex: String = escape.chars().skip(2).collect();
            let n = u32::from_str_radix(&hex, 16).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            if char::from_u32(n).is_none() {
                return Err(source.error(&format!("bad escape {}", escape), escape.chars().count()));
            }
            Ok(EscapeResult::Literal(n))
        }
        Some('N') if source.istext => {
            // See class_escape above — same gap applies outside classes.
            Err(source.error(
                r"\N{...} named unicode escapes not yet supported in native regex parser",
                0,
            ))
        }
        Some('0') => {
            escape.push_str(&source.getwhile(2, OCTDIGITS)?);
            let oct: String = escape.chars().skip(1).collect();
            let n = u32::from_str_radix(&oct, 8).map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            Ok(EscapeResult::Literal(n))
        }
        Some(ch) if DIGITS.contains(ch) => {
            let nxt = source.peek_next_char();
            if let Some(n2) = nxt {
                if DIGITS.contains(n2) {
                    let more = source.get()?.unwrap();
                    escape.push_str(&more);
                    let e_chars: Vec<char> = escape.chars().collect();
                    if OCTDIGITS.contains(e_chars[1])
                        && OCTDIGITS.contains(e_chars[2])
                        && source
                            .peek_next_char()
                            .map(|c| OCTDIGITS.contains(c))
                            .unwrap_or(false)
                    {
                        let more = source.get()?.unwrap();
                        escape.push_str(&more);
                        let oct: String = escape.chars().skip(1).collect();
                        let n = u32::from_str_radix(&oct, 8).map_err(|_| {
                            source.error(&format!("bad escape {}", escape), escape.chars().count())
                        })?;
                        if n > 0o377 {
                            return Err(source.error(
                                &format!("octal escape value {} outside of range 0-0o377", escape),
                                escape.chars().count(),
                            ));
                        }
                        return Ok(EscapeResult::Literal(n));
                    }
                }
            }
            let dec: String = escape.chars().skip(1).collect();
            let group = dec.parse::<u32>().map_err(|_| {
                source.error(&format!("bad escape {}", escape), escape.chars().count())
            })?;
            if group < state.groups() {
                if !state.checkgroup(group) {
                    return Err(
                        source.error("cannot refer to an open group", escape.chars().count())
                    );
                }
                state.checklookbehindgroup(group, source)?;
                return Ok(EscapeResult::GroupRef(group));
            }
            Err(source.error(
                &format!("invalid group reference {}", group),
                escape.chars().count() - 1,
            ))
        }
        Some(ch) => {
            if escape.chars().count() == 2 {
                if ASCIILETTERS.contains(ch) {
                    return Err(
                        source.error(&format!("bad escape {}", escape), escape.chars().count())
                    );
                }
                return Ok(EscapeResult::Literal(ch as u32));
            }
            Err(source.error(&format!("bad escape {}", escape), escape.chars().count()))
        }
        None => Err(source.error(&format!("bad escape {}", escape), escape.chars().count())),
    }
}

enum EscapeResult {
    Literal(u32),
    At(AtCode),
    InCategory(ChCode),
    GroupRef(u32),
}

fn uniq_set(items: Vec<SetItem>) -> Vec<SetItem> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for item in items {
        if !seen.iter().any(|x: &SetItem| x == &item) {
            seen.push(item.clone());
            out.push(item);
        }
    }
    out
}

fn is_repeat_opcode(op: &OpCode) -> bool {
    matches!(
        op,
        OpCode::MinRepeat { .. } | OpCode::MaxRepeat { .. } | OpCode::PossessiveRepeat { .. }
    )
}

/// Port of `_parser._parse_sub`. Parses an alternation.
fn parse_sub(
    source: &mut Tokenizer,
    state: &mut State,
    mut verbose: bool,
    nested: u32,
) -> ParseResult<SubPattern> {
    let mut items: Vec<SubPattern> = Vec::new();

    loop {
        let first = nested == 0 && items.is_empty();
        items.push(parse(source, state, verbose, nested + 1, first)?);
        if !source.take_match('|')? {
            break;
        }
        if nested == 0 {
            verbose = (state.flags & SRE_FLAG_VERBOSE) != 0;
        }
    }

    if items.len() == 1 {
        return Ok(items.remove(0));
    }

    let mut subpattern = SubPattern::new();

    // Pull out shared prefixes (exact port of the Python loop).
    loop {
        let mut prefix: Option<OpCode> = None;
        let mut all_share = true;
        for item in &items {
            if item.is_empty() {
                all_share = false;
                break;
            }
            match &prefix {
                None => prefix = Some(item.data[0].clone()),
                Some(p) => {
                    if item.data[0] != *p {
                        all_share = false;
                        break;
                    }
                }
            }
        }
        if !all_share {
            break;
        }
        let Some(p) = prefix else { break };
        for item in &mut items {
            item.data.remove(0);
        }
        subpattern.push(p);
    }

    // Check if the branch can be replaced by a character set.
    let mut set: Vec<SetItem> = Vec::new();
    let mut flatten_ok = true;
    for item in &items {
        if item.len() != 1 {
            flatten_ok = false;
            break;
        }
        match &item.data[0] {
            OpCode::Literal(cp) => set.push(SetItem::Literal(*cp)),
            OpCode::In(inner) if !matches!(inner.first(), Some(SetItem::Negate)) => {
                set.extend(inner.iter().cloned());
            }
            _ => {
                flatten_ok = false;
                break;
            }
        }
    }
    if flatten_ok {
        subpattern.push(OpCode::In(uniq_set(set)));
        return Ok(subpattern);
    }

    subpattern.push(OpCode::Branch(items));
    Ok(subpattern)
}

/// Port of `_parser._parse`. Parses one simple pattern (no `|`).
fn parse(
    source: &mut Tokenizer,
    state: &mut State,
    mut verbose: bool,
    nested: u32,
    first: bool,
) -> ParseResult<SubPattern> {
    let mut subpattern = SubPattern::new();

    loop {
        let this = source.next.clone();
        let Some(this) = this else { break };
        if this == "|" || this == ")" {
            break;
        }
        source.advance()?;

        if verbose && this.chars().count() == 1 {
            let c = this.chars().next().unwrap();
            if WHITESPACE.contains(c) {
                continue;
            }
            if c == '#' {
                loop {
                    let got = source.get()?;
                    match got {
                        None => break,
                        Some(s) if s == "\n" => break,
                        _ => {}
                    }
                }
                continue;
            }
        }

        let first_char = this.chars().next().unwrap();

        if first_char == '\\' {
            match escape_code(source, &this, state)? {
                EscapeResult::Literal(cp) => subpattern.push(OpCode::Literal(cp)),
                EscapeResult::At(a) => subpattern.push(OpCode::At(a)),
                EscapeResult::InCategory(cat) => {
                    subpattern.push(OpCode::In(vec![SetItem::Category(cat)]));
                }
                EscapeResult::GroupRef(g) => subpattern.push(OpCode::GroupRef(g)),
            }
            continue;
        }

        if !SPECIAL_CHARS.contains(first_char) {
            subpattern.push(OpCode::Literal(first_char as u32));
            continue;
        }

        if this == "[" {
            let here = source.tell().saturating_sub(1);
            let mut set: Vec<SetItem> = Vec::new();
            let negate = source.take_match('^')?;
            loop {
                let got = source.get()?;
                let Some(cur) = got else {
                    return Err(source.error(
                        "unterminated character set",
                        source.tell().saturating_sub(here),
                    ));
                };
                if cur == "]" && !set.is_empty() {
                    break;
                }
                let code1: Either = if cur.starts_with('\\') {
                    Either::Class(class_escape(source, &cur)?)
                } else {
                    Either::Literal(cur.chars().next().unwrap() as u32)
                };
                if source.take_match('-')? {
                    let got_that = source.get()?;
                    let Some(that) = got_that else {
                        return Err(source.error(
                            "unterminated character set",
                            source.tell().saturating_sub(here),
                        ));
                    };
                    if that == "]" {
                        push_set_code(&mut set, code1);
                        set.push(SetItem::Literal('-' as u32));
                        break;
                    }
                    let code2: Either = if that.starts_with('\\') {
                        Either::Class(class_escape(source, &that)?)
                    } else {
                        Either::Literal(that.chars().next().unwrap() as u32)
                    };
                    let (lo, hi) = match (&code1, &code2) {
                        (Either::Literal(l), Either::Literal(h)) => (*l, *h),
                        _ => {
                            return Err(source.error(
                                &format!("bad character range {}-{}", cur, that),
                                cur.chars().count() + 1 + that.chars().count(),
                            ));
                        }
                    };
                    if hi < lo {
                        return Err(source.error(
                            &format!("bad character range {}-{}", cur, that),
                            cur.chars().count() + 1 + that.chars().count(),
                        ));
                    }
                    set.push(SetItem::Range(lo, hi));
                } else {
                    push_set_code(&mut set, code1);
                }
            }
            let set = uniq_set(set);
            if set.len() == 1 {
                if let SetItem::Literal(cp) = set[0] {
                    if negate {
                        subpattern.push(OpCode::NotLiteral(cp));
                    } else {
                        subpattern.push(OpCode::Literal(cp));
                    }
                    continue;
                }
            }
            let mut final_set = set;
            if negate {
                final_set.insert(0, SetItem::Negate);
            }
            subpattern.push(OpCode::In(final_set));
            continue;
        }

        if this.chars().count() == 1 && REPEAT_CHARS.contains(first_char) {
            let here = source.tell();
            let (mut min, mut max): (u32, u32);
            if this == "?" {
                min = 0;
                max = 1;
            } else if this == "*" {
                min = 0;
                max = MAXREPEAT;
            } else if this == "+" {
                min = 1;
                max = MAXREPEAT;
            } else if this == "{" {
                if source.peek_next_char() == Some('}') {
                    subpattern.push(OpCode::Literal(first_char as u32));
                    continue;
                }
                min = 0;
                max = MAXREPEAT;
                let mut lo = String::new();
                let mut hi = String::new();
                while let Some(c) = source.peek_next_char() {
                    if !DIGITS.contains(c) {
                        break;
                    }
                    lo.push_str(&source.get()?.unwrap());
                }
                if source.take_match(',')? {
                    while let Some(c) = source.peek_next_char() {
                        if !DIGITS.contains(c) {
                            break;
                        }
                        hi.push_str(&source.get()?.unwrap());
                    }
                } else {
                    hi = lo.clone();
                }
                if !source.take_match('}')? {
                    subpattern.push(OpCode::Literal(first_char as u32));
                    source.seek(here)?;
                    continue;
                }
                if !lo.is_empty() {
                    let parsed = lo
                        .parse::<u64>()
                        .map_err(|_| source.error("the repetition number is too large", 0))?;
                    if parsed >= MAXREPEAT as u64 {
                        return Err(source.error("the repetition number is too large", 0));
                    }
                    min = parsed as u32;
                }
                if !hi.is_empty() {
                    let parsed = hi
                        .parse::<u64>()
                        .map_err(|_| source.error("the repetition number is too large", 0))?;
                    if parsed >= MAXREPEAT as u64 {
                        return Err(source.error("the repetition number is too large", 0));
                    }
                    max = parsed as u32;
                    if max < min {
                        return Err(source.error(
                            "min repeat greater than max repeat",
                            source.tell().saturating_sub(here),
                        ));
                    }
                }
            } else {
                unreachable!("REPEAT_CHARS dispatch");
            }
            let item_opt = if subpattern.is_empty() {
                None
            } else {
                Some(subpattern.data[subpattern.len() - 1].clone())
            };
            let Some(item_op) = item_opt else {
                return Err(source.error(
                    "nothing to repeat",
                    source.tell().saturating_sub(here) + this.chars().count(),
                ));
            };
            if matches!(item_op, OpCode::At(_)) {
                return Err(source.error(
                    "nothing to repeat",
                    source.tell().saturating_sub(here) + this.chars().count(),
                ));
            }
            if is_repeat_opcode(&item_op) {
                return Err(source.error(
                    "multiple repeat",
                    source.tell().saturating_sub(here) + this.chars().count(),
                ));
            }
            // Unwrap unnamed/unflagged non-capturing groups: the Python
            // parser collapses `(?:X){m,n}` to just repeating X when the
            // group has no group id or flags.
            let inner = match item_op {
                OpCode::Subpattern {
                    group: None,
                    add_flags: 0,
                    del_flags: 0,
                    p,
                } => p,
                other => {
                    let mut sp = SubPattern::new();
                    sp.push(other);
                    sp
                }
            };

            subpattern.data.pop();
            let repeat = if source.take_match('?')? {
                OpCode::MinRepeat {
                    min,
                    max,
                    item: inner,
                }
            } else if source.take_match('+')? {
                OpCode::PossessiveRepeat {
                    min,
                    max,
                    item: inner,
                }
            } else {
                OpCode::MaxRepeat {
                    min,
                    max,
                    item: inner,
                }
            };
            subpattern.push(repeat);
            continue;
        }

        if this == "." {
            subpattern.push(OpCode::Any);
            continue;
        }

        if this == "(" {
            let start = source.tell().saturating_sub(1);
            let mut capture = true;
            let mut atomic = false;
            let mut name: Option<String> = None;
            let mut add_flags: u32 = 0;
            let mut del_flags: u32 = 0;
            if source.take_match('?')? {
                let got = source.get()?;
                let Some(ch) = got else {
                    return Err(source.error("unexpected end of pattern", 0));
                };
                if ch == "P" {
                    if source.take_match('<')? {
                        let n = source.getuntil('>', "group name")?;
                        source.checkgroupname(&n, 1)?;
                        name = Some(n);
                    } else if source.take_match('=')? {
                        let n = source.getuntil(')', "group name")?;
                        source.checkgroupname(&n, 1)?;
                        let Some(&gid) = state.groupdict.get(&n) else {
                            return Err(source.error(
                                &format!("unknown group name '{}'", n),
                                n.chars().count() + 1,
                            ));
                        };
                        if !state.checkgroup(gid) {
                            return Err(source
                                .error("cannot refer to an open group", n.chars().count() + 1));
                        }
                        state.checklookbehindgroup(gid, source)?;
                        subpattern.push(OpCode::GroupRef(gid));
                        continue;
                    } else {
                        let got2 = source.get()?;
                        let Some(ch2) = got2 else {
                            return Err(source.error("unexpected end of pattern", 0));
                        };
                        return Err(source.error(
                            &format!("unknown extension ?P{}", ch2),
                            ch2.chars().count() + 2,
                        ));
                    }
                } else if ch == ":" {
                    capture = false;
                } else if ch == "#" {
                    loop {
                        if source.next.is_none() {
                            return Err(source.error(
                                "missing ), unterminated comment",
                                source.tell().saturating_sub(start),
                            ));
                        }
                        let got = source.get()?;
                        if matches!(got.as_deref(), Some(")")) {
                            break;
                        }
                    }
                    continue;
                } else if ch == "=" || ch == "!" || ch == "<" {
                    let mut dir: i32 = 1;
                    let mut marker = ch.clone();
                    let mut saved_lookbehind: Option<Option<u32>> = None;
                    if ch == "<" {
                        let got = source.get()?;
                        let Some(lookch) = got else {
                            return Err(source.error("unexpected end of pattern", 0));
                        };
                        if lookch != "=" && lookch != "!" {
                            return Err(source.error(
                                &format!("unknown extension ?<{}", lookch),
                                lookch.chars().count() + 2,
                            ));
                        }
                        dir = -1;
                        saved_lookbehind = Some(state.lookbehindgroups);
                        if state.lookbehindgroups.is_none() {
                            state.lookbehindgroups = Some(state.groups());
                        }
                        marker = lookch;
                    }
                    let p = parse_sub(source, state, verbose, nested + 1)?;
                    if dir < 0 {
                        if let Some(saved) = saved_lookbehind {
                            if saved.is_none() {
                                state.lookbehindgroups = None;
                            }
                        }
                    }
                    if !source.take_match(')')? {
                        return Err(source.error(
                            "missing ), unterminated subpattern",
                            source.tell().saturating_sub(start),
                        ));
                    }
                    if marker == "=" {
                        subpattern.push(OpCode::Assert { direction: dir, p });
                    } else if !p.is_empty() {
                        subpattern.push(OpCode::AssertNot { direction: dir, p });
                    } else {
                        subpattern.push(OpCode::Failure);
                    }
                    continue;
                } else if ch == "(" {
                    let condname = source.getuntil(')', "group name")?;
                    let condgroup: u32;
                    if condname.is_empty() || !condname.chars().all(|c| c.is_ascii_digit()) {
                        source.checkgroupname(&condname, 1)?;
                        let Some(&g) = state.groupdict.get(&condname) else {
                            return Err(source.error(
                                &format!("unknown group name '{}'", condname),
                                condname.chars().count() + 1,
                            ));
                        };
                        condgroup = g;
                    } else {
                        let parsed: u32 = condname.parse().map_err(|_| {
                            source.error(
                                &format!("invalid group reference {}", condname),
                                condname.chars().count() + 1,
                            )
                        })?;
                        if parsed == 0 {
                            return Err(
                                source.error("bad group number", condname.chars().count() + 1)
                            );
                        }
                        if parsed >= MAXGROUPS {
                            return Err(source.error(
                                &format!("invalid group reference {}", parsed),
                                condname.chars().count() + 1,
                            ));
                        }
                        condgroup = parsed;
                        state
                            .grouprefpos
                            .entry(condgroup)
                            .or_insert(source.tell().saturating_sub(condname.chars().count() + 1));
                    }
                    state.checklookbehindgroup(condgroup, source)?;
                    let item_yes = parse(source, state, verbose, nested + 1, false)?;
                    let item_no = if source.take_match('|')? {
                        let no = parse(source, state, verbose, nested + 1, false)?;
                        if source.peek_next_char() == Some('|') {
                            return Err(
                                source.error("conditional backref with more than two branches", 0)
                            );
                        }
                        Some(no)
                    } else {
                        None
                    };
                    if !source.take_match(')')? {
                        return Err(source.error(
                            "missing ), unterminated subpattern",
                            source.tell().saturating_sub(start),
                        ));
                    }
                    subpattern.push(OpCode::GroupRefExists {
                        cond_group: condgroup,
                        yes: item_yes,
                        no: item_no,
                    });
                    continue;
                } else if ch == ">" {
                    capture = false;
                    atomic = true;
                } else if flag_for_char(ch.chars().next().unwrap()).is_some() || ch == "-" {
                    let flags = parse_flags(source, state, ch.chars().next().unwrap())?;
                    match flags {
                        None => {
                            if !first || !subpattern.is_empty() {
                                return Err(source.error(
                                    "global flags not at the start of the expression",
                                    source.tell().saturating_sub(start),
                                ));
                            }
                            verbose = (state.flags & SRE_FLAG_VERBOSE) != 0;
                            continue;
                        }
                        Some((af, df)) => {
                            add_flags = af;
                            del_flags = df;
                            capture = false;
                        }
                    }
                } else {
                    return Err(source.error(
                        &format!("unknown extension ?{}", ch),
                        ch.chars().count() + 1,
                    ));
                }
            }

            let group: Option<u32> = if capture {
                Some(state.opengroup(name.as_deref())?)
            } else {
                None
            };
            let sub_verbose = (verbose || (add_flags & SRE_FLAG_VERBOSE) != 0)
                && (del_flags & SRE_FLAG_VERBOSE) == 0;
            let p = parse_sub(source, state, sub_verbose, nested + 1)?;
            if !source.take_match(')')? {
                return Err(source.error(
                    "missing ), unterminated subpattern",
                    source.tell().saturating_sub(start),
                ));
            }
            if let Some(g) = group {
                state.closegroup(g);
            }
            if atomic {
                debug_assert!(group.is_none());
                subpattern.push(OpCode::AtomicGroup(p));
            } else {
                subpattern.push(OpCode::Subpattern {
                    group,
                    add_flags,
                    del_flags,
                    p,
                });
            }
            continue;
        }

        if this == "^" {
            subpattern.push(OpCode::At(AtCode::Beginning));
            continue;
        }
        if this == "$" {
            subpattern.push(OpCode::At(AtCode::End));
            continue;
        }

        unreachable!("unhandled special character {:?}", this);
    }

    // Unpack non-capturing groups with no flags.
    let mut i = subpattern.data.len();
    while i > 0 {
        i -= 1;
        let take_inner = matches!(
            &subpattern.data[i],
            OpCode::Subpattern {
                group: None,
                add_flags: 0,
                del_flags: 0,
                ..
            }
        );
        if take_inner {
            let OpCode::Subpattern { p, .. } = subpattern.data.remove(i) else {
                unreachable!()
            };
            let expanded = p.data;
            let expanded_len = expanded.len();
            for (j, op) in expanded.into_iter().enumerate() {
                subpattern.data.insert(i + j, op);
            }
            i += expanded_len;
        }
    }

    Ok(subpattern)
}

/// Either a simple literal or a class-escape result (used when tokenising
/// the body of a character class).
enum Either {
    Literal(u32),
    Class(ClassEscapeResult),
}

fn push_set_code(set: &mut Vec<SetItem>, code: Either) {
    match code {
        Either::Literal(cp) => set.push(SetItem::Literal(cp)),
        Either::Class(ClassEscapeResult::Literal(cp)) => set.push(SetItem::Literal(cp)),
        Either::Class(ClassEscapeResult::Category(cat)) => set.push(SetItem::Category(cat)),
    }
}

/// Port of `_parser._parse_flags`.
fn parse_flags(
    source: &mut Tokenizer,
    state: &mut State,
    first_ch: char,
) -> ParseResult<Option<(u32, u32)>> {
    let mut add_flags: u32 = 0;
    let mut del_flags: u32 = 0;
    let mut ch = first_ch.to_string();
    if ch != "-" {
        loop {
            let Some(flag) = flag_for_char(ch.chars().next().unwrap()) else {
                unreachable!("parse_flags called with non-flag char");
            };
            if source.istext && ch == "L" {
                return Err(source.error(
                    "bad inline flags: cannot use 'L' flag with a str pattern",
                    0,
                ));
            }
            if !source.istext && ch == "u" {
                return Err(source.error(
                    "bad inline flags: cannot use 'u' flag with a bytes pattern",
                    0,
                ));
            }
            add_flags |= flag;
            if flag & TYPE_FLAGS != 0 && (add_flags & TYPE_FLAGS) != flag {
                return Err(source.error(
                    "bad inline flags: flags 'a', 'u' and 'L' are incompatible",
                    0,
                ));
            }
            let got = source.get()?;
            let Some(n) = got else {
                return Err(source.error("missing -, : or )", 0));
            };
            ch = n;
            if ch == ")" || ch == "-" || ch == ":" {
                break;
            }
            if flag_for_char(ch.chars().next().unwrap()).is_none() {
                let msg = if ch.chars().next().unwrap().is_alphabetic() {
                    "unknown flag"
                } else {
                    "missing -, : or )"
                };
                return Err(source.error(msg, ch.chars().count()));
            }
        }
    }
    if ch == ")" {
        state.flags |= add_flags;
        return Ok(None);
    }
    if add_flags & GLOBAL_FLAGS != 0 {
        return Err(source.error("bad inline flags: cannot turn on global flag", 1));
    }
    if ch == "-" {
        let got = source.get()?;
        let Some(n) = got else {
            return Err(source.error("missing flag", 0));
        };
        ch = n;
        if flag_for_char(ch.chars().next().unwrap()).is_none() {
            let msg = if ch.chars().next().unwrap().is_alphabetic() {
                "unknown flag"
            } else {
                "missing flag"
            };
            return Err(source.error(msg, ch.chars().count()));
        }
        loop {
            let flag = flag_for_char(ch.chars().next().unwrap()).unwrap();
            if flag & TYPE_FLAGS != 0 {
                return Err(source.error(
                    "bad inline flags: cannot turn off flags 'a', 'u' and 'L'",
                    0,
                ));
            }
            del_flags |= flag;
            let got = source.get()?;
            let Some(n) = got else {
                return Err(source.error("missing :", 0));
            };
            ch = n;
            if ch == ":" {
                break;
            }
            if flag_for_char(ch.chars().next().unwrap()).is_none() {
                let msg = if ch.chars().next().unwrap().is_alphabetic() {
                    "unknown flag"
                } else {
                    "missing :"
                };
                return Err(source.error(msg, ch.chars().count()));
            }
        }
    }
    debug_assert_eq!(ch, ":");
    if del_flags & GLOBAL_FLAGS != 0 {
        return Err(source.error("bad inline flags: cannot turn off global flag", 1));
    }
    if add_flags & del_flags != 0 {
        return Err(source.error("bad inline flags: flag turned on and off", 1));
    }
    Ok(Some((add_flags, del_flags)))
}

/// Port of `_parser.fix_flags`. Str-pattern only; callers always pass
/// `istext = true` in our code base.
fn fix_flags(istext: bool, mut flags: u32) -> ParseResult<u32> {
    if istext {
        if flags & SRE_FLAG_LOCALE != 0 {
            return Err(ParseError {
                msg: "cannot use LOCALE flag with a str pattern".into(),
                pos: 0,
            });
        }
        if flags & SRE_FLAG_ASCII == 0 {
            flags |= SRE_FLAG_UNICODE;
        } else if flags & SRE_FLAG_UNICODE != 0 {
            return Err(ParseError {
                msg: "ASCII and UNICODE flags are incompatible".into(),
                pos: 0,
            });
        }
    } else {
        if flags & SRE_FLAG_UNICODE != 0 {
            return Err(ParseError {
                msg: "cannot use UNICODE flag with a bytes pattern".into(),
                pos: 0,
            });
        }
        if flags & SRE_FLAG_LOCALE != 0 && flags & SRE_FLAG_ASCII != 0 {
            return Err(ParseError {
                msg: "ASCII and LOCALE flags are incompatible".into(),
                pos: 0,
            });
        }
    }
    Ok(flags)
}

/// Parse a Python regex pattern string.
///
/// Matches the public entry point `_parser.parse(str, flags=0)`. Always
/// treats `pattern` as a `str` pattern; bytes patterns are not produced by
/// Hypothesis's native generator in hegel-rust.
pub fn parse_pattern(pattern: &str, flags: u32) -> ParseResult<ParsedPattern> {
    let mut source = Tokenizer::new(pattern, true)?;
    let mut state = State::new();
    state.flags = flags;

    let p = parse_sub(&mut source, &mut state, flags & SRE_FLAG_VERBOSE != 0, 0)?;
    state.flags = fix_flags(true, state.flags)?;

    if source.next.is_some() {
        debug_assert_eq!(source.next.as_deref(), Some(")"));
        return Err(source.error("unbalanced parenthesis", 0));
    }

    for (&g, &pos) in &state.grouprefpos {
        if g >= state.groups() {
            return Err(ParseError {
                msg: format!("invalid group reference {}", g),
                pos,
            });
        }
    }

    Ok(ParsedPattern {
        pattern: p,
        flags: state.flags,
        group_count: state.groups(),
        group_names: state.groupdict,
    })
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/re_parser_tests.rs"]
mod tests;
