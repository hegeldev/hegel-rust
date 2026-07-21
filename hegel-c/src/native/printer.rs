//! A pretty-printer with deferred and speculative regions and line comments.
//!
//! The line-breaking core is a port of `hypothesis.vendor.pretty` (itself a
//! fork of `IPython.lib.pretty`, based on Ruby's `prettyprint.rb`
//! implementation of Oppen's prettyprinting algorithm in simplified form).
//! Output is built from three primitives: [`Printer::text`] emits unbreakable
//! text, [`Printer::breakable`] marks a point that renders as a separator if
//! the enclosing group fits on the line and as a newline plus indentation if
//! it does not, and [`Printer::begin_group`]/[`Printer::end_group`] delimit
//! the groups those decisions are made over. Breaking is all-or-nothing per
//! group and decided eagerly, outermost groups first, the moment buffered
//! output would exceed the maximum width.
//!
//! Two deliberate deviations from the Python original: `GroupQueue.deq`
//! iterated each depth level in reverse but deleted by forward index (only
//! self-consistent for single-entry levels) — here groups at a level are
//! considered oldest first; and `flush` accumulated the returned column with
//! `+=` where every other caller assigned it — here the column is always
//! assigned. The object-identity machinery (`known_object_printers`,
//! singleton printers, cycle detection by `id()`) is intentionally not
//! ported: representations are produced at generation time by the client, so
//! there is nothing to look up after the fact.
//!
//! Commands are buffered as they arrive and the document is laid out from
//! scratch on every [`Printer::value`] call. Rendering a complete command
//! list rather than streaming makes [`Printer::comment`] possible: a comment
//! attaches to the line being written and is emitted verbatim immediately
//! before that line's newline (or at the end of the document), it contributes
//! nothing to width accounting, and every group open at the comment's
//! position is forced to break — including break points seen before the
//! comment — because anything after a comment on its line would become part
//! of the comment. A comment-forced group also breaks before its closing
//! text (with leading whitespace trimmed from the close), so trailing
//! delimiters are not annotated by a comment on the group's last element.
//! The printer stores comment text verbatim: clients supply the full
//! rendered form, including the comment syntax of the language they are
//! printing for and any separating whitespace.
//!
//! On top of the core sits a recording layer, ported from the deferred
//! printing work in Hypothesis (`RepresentationPrinter.deferred`/`resolve`).
//! [`Printer::deferred`] opens a hole in the output and returns a [`SlotId`];
//! content written to the slot later — while the test body runs — is spliced
//! in at the hole's position, and because the document is laid out over the
//! complete content, line-breaking behaves exactly as if everything had been
//! printed inline. [`Printer::resolve`] closes the outstanding session:
//! every slot of the session dies and layout errors in its content surface.
//! [`Printer::begin_speculative`] buffers output that may be retracted:
//! commands are held until [`Printer::commit_speculative`] appends them to
//! their target or [`Printer::abort_speculative`] discards them, which is
//! how draw-time printing survives rejection (filters, collection rejection,
//! failed assumptions).
//!
//! Text passed to [`Printer::text`] and [`Printer::comment`] must not
//! contain newlines; use [`Printer::hard_break`] instead so column and
//! indentation accounting stay correct. Widths are counted in `char`s.

use std::collections::VecDeque;

/// An error from a [`Printer`] operation. Every error reports API misuse;
/// well-formed printing never fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterError {
    /// A [`SlotId`] was used after the session that created it was resolved,
    /// or after the speculative region containing it was aborted.
    DeadSlot,
    /// `end_group` was executed with no group open.
    UnbalancedGroup,
    /// `commit_speculative` or `abort_speculative` was called with no open
    /// speculative region on the target.
    NoSpeculation,
    /// `resolve` or `value` was called while a speculative region was open.
    OpenSpeculation,
    /// `value` was called while a deferred session was outstanding.
    UnresolvedDeferred,
    /// `resolve` was called with no deferred session outstanding.
    NothingToResolve,
}

impl std::fmt::Display for PrinterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PrinterError::DeadSlot => "deferred slot used after its printing session ended",
            PrinterError::UnbalancedGroup => "end_group without a matching begin_group",
            PrinterError::NoSpeculation => "commit or abort without an open speculative region",
            PrinterError::OpenSpeculation => {
                "operation requires all speculative regions to be closed"
            }
            PrinterError::UnresolvedDeferred => "printer has unresolved deferred slots",
            PrinterError::NothingToResolve => "resolve called with no outstanding deferred slots",
        })
    }
}

impl std::error::Error for PrinterError {}

/// A handle to a deferred hole opened by [`Printer::deferred`]. Content
/// written to the slot is spliced in at the hole's position; after
/// [`Printer::resolve`] the slot is dead and all further writes error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotId(usize);

/// Where a [`Printer`] operation writes: the main output, or a deferred slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The printer's main output.
    Main,
    /// The deferred slot with this id.
    Slot(SlotId),
}

#[derive(Debug, Clone)]
enum Cmd {
    Text(String),
    Breakable(String),
    IfBreak(String),
    HardBreak,
    BeginGroup { indent: usize, open: String },
    EndGroup { close: String },
    ShiftIndent(isize),
    Comment(String),
    Splice(SlotId),
}

#[derive(Debug)]
enum Token {
    Text {
        content: String,
        width: usize,
    },
    Breakable {
        sep: String,
        width: usize,
        indent: isize,
        group: usize,
    },
    IfBreak {
        content: String,
        group: usize,
    },
    Comment {
        content: String,
    },
}

impl Token {
    fn width(&self) -> usize {
        match self {
            Token::Text { width, .. } | Token::Breakable { width, .. } => *width,
            Token::IfBreak { .. } | Token::Comment { .. } => 0,
        }
    }
}

#[derive(Debug)]
struct Group {
    depth: usize,
    indent: usize,
    pending: usize,
    want_break: bool,
    comment: bool,
}

#[derive(Debug, Default)]
struct Slot {
    commands: Vec<Cmd>,
    speculation: Vec<Vec<Cmd>>,
    dead: bool,
}

fn spaces(n: isize) -> String {
    " ".repeat(n.max(0) as usize)
}

fn splice_ids(cmds: &[Cmd]) -> Vec<usize> {
    cmds.iter()
        .filter_map(|cmd| match cmd {
            Cmd::Splice(SlotId(id)) => Some(*id),
            _ => None,
        })
        .collect()
}

fn mark_comment_groups(
    slots: &[Slot],
    cmds: &[Cmd],
    force: &mut Vec<bool>,
    stack: &mut Vec<usize>,
) {
    for cmd in cmds {
        match cmd {
            Cmd::BeginGroup { .. } => {
                force.push(false);
                stack.push(force.len() - 1);
            }
            Cmd::EndGroup { .. } => {
                stack.pop();
            }
            Cmd::Comment(_) => {
                for &id in stack.iter() {
                    force[id] = true;
                }
            }
            Cmd::Splice(SlotId(id)) => {
                mark_comment_groups(slots, &slots[*id].commands, force, stack);
            }
            _ => {}
        }
    }
}

/// The pretty printer. See the module docs for the printing model.
#[derive(Debug)]
pub struct Printer {
    max_width: usize,
    main: Vec<Cmd>,
    open_groups: isize,
    speculation: Vec<Vec<Cmd>>,
    slots: Vec<Slot>,
    pending_resolve: bool,
    rendered: String,
}

impl Printer {
    /// Create a printer that tries to keep lines within `max_width` chars.
    pub fn new(max_width: usize) -> Printer {
        Printer {
            max_width,
            main: Vec::new(),
            open_groups: 0,
            speculation: Vec::new(),
            slots: Vec::new(),
            pending_resolve: false,
            rendered: String::new(),
        }
    }

    /// Emit literal, unbreakable text. Must not contain newlines.
    ///
    /// Group-balance misuse (an `end_group` with no group open) is reported
    /// eagerly while the document has no deferred slots; once a slot exists,
    /// group structure may legitimately span holes, so imbalance surfaces at
    /// [`Printer::resolve`] or [`Printer::value`] instead.
    pub fn text(&mut self, target: Target, s: &str) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::Text(s.to_string()))
    }

    /// Emit a potential break point: renders as `sep` if the enclosing group
    /// fits on the line, and as a newline plus the current indentation if the
    /// group breaks.
    /// Emit `s` only if the innermost group open at this point renders
    /// broken; a group that fits on one line renders nothing here. `s` never
    /// counts toward width (measurement uses the flat form, which is empty).
    /// This is how a layout expresses text that only the multi-line form
    /// needs — e.g. Go's mandatory trailing comma before a composite
    /// literal's closing brace.
    pub fn if_break(&mut self, target: Target, s: &str) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::IfBreak(s.to_string()))
    }

    pub fn breakable(&mut self, target: Target, sep: &str) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::Breakable(sep.to_string()))
    }

    /// Emit an unconditional newline followed by the current indentation.
    pub fn hard_break(&mut self, target: Target) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::HardBreak)
    }

    /// Open a group: emit `open`, then increase the indentation by `indent`.
    /// Break decisions are made per group; see the module docs.
    pub fn begin_group(
        &mut self,
        target: Target,
        indent: usize,
        open: &str,
    ) -> Result<(), PrinterError> {
        self.dispatch(
            target,
            Cmd::BeginGroup {
                indent,
                open: open.to_string(),
            },
        )
    }

    /// Close the innermost group: undo the indentation its `begin_group`
    /// added, then emit `close`.
    pub fn end_group(&mut self, target: Target, close: &str) -> Result<(), PrinterError> {
        self.dispatch(
            target,
            Cmd::EndGroup {
                close: close.to_string(),
            },
        )
    }

    /// Adjust the indentation applied by subsequent break points by `delta`.
    pub fn shift_indent(&mut self, target: Target, delta: isize) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::ShiftIndent(delta))
    }

    /// Attach a comment to the line currently being written to `target`: the
    /// text is emitted verbatim at the end of that line, every group open at
    /// this position breaks, and the comment is excluded from width
    /// accounting. The text must be the full rendered form of the comment
    /// (e.g. `"  // like this"`) and must not contain newlines.
    pub fn comment(&mut self, target: Target, s: &str) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::Comment(s.to_string()))
    }

    /// Open a deferred hole at the current position of `target` and return
    /// its slot. Writes to the slot are spliced in at the hole's position.
    pub fn deferred(&mut self, target: Target) -> Result<SlotId, PrinterError> {
        if let Target::Slot(SlotId(id)) = target {
            if self.slots[id].dead {
                return Err(PrinterError::DeadSlot);
            }
        }
        let slot = SlotId(self.slots.len());
        self.slots.push(Slot::default());
        self.dispatch(target, Cmd::Splice(slot))?;
        Ok(slot)
    }

    /// Open a speculative region on `target`: subsequent writes to it buffer
    /// until [`Printer::commit_speculative`] emits them or
    /// [`Printer::abort_speculative`] discards them.
    pub fn begin_speculative(&mut self, target: Target) -> Result<(), PrinterError> {
        match target {
            Target::Main => self.speculation.push(Vec::new()),
            Target::Slot(SlotId(id)) => {
                let slot = &mut self.slots[id];
                if slot.dead {
                    return Err(PrinterError::DeadSlot);
                }
                slot.speculation.push(Vec::new());
            }
        }
        Ok(())
    }

    /// Close the innermost speculative region on `target`, keeping its
    /// content. Committing into the main output validates group balance and
    /// rejects atomically, leaving the region open.
    pub fn commit_speculative(&mut self, target: Target) -> Result<(), PrinterError> {
        match target {
            Target::Main => {
                if self.speculation.is_empty() {
                    return Err(PrinterError::NoSpeculation);
                }
                if self.speculation.len() > 1 {
                    let buf = self.speculation.pop().unwrap();
                    self.speculation.last_mut().unwrap().extend(buf);
                    return Ok(());
                }
                let sound = self.slots.is_empty();
                let mut balance = self.open_groups;
                for cmd in self.speculation.last().unwrap() {
                    match cmd {
                        Cmd::BeginGroup { .. } => balance += 1,
                        Cmd::EndGroup { .. } => {
                            balance -= 1;
                            if sound && balance < 0 {
                                return Err(PrinterError::UnbalancedGroup);
                            }
                        }
                        _ => {}
                    }
                }
                let buf = self.speculation.pop().unwrap();
                if buf.iter().any(|cmd| matches!(cmd, Cmd::Splice(_))) {
                    self.pending_resolve = true;
                }
                self.open_groups = balance;
                self.main.extend(buf);
            }
            Target::Slot(SlotId(id)) => {
                let slot = &mut self.slots[id];
                if slot.dead {
                    return Err(PrinterError::DeadSlot);
                }
                let buf = slot.speculation.pop().ok_or(PrinterError::NoSpeculation)?;
                if let Some(outer) = slot.speculation.last_mut() {
                    outer.extend(buf);
                } else {
                    slot.commands.extend(buf);
                }
            }
        }
        Ok(())
    }

    /// Close the innermost speculative region on `target`, discarding its
    /// content. Deferred slots opened inside the region die with it.
    pub fn abort_speculative(&mut self, target: Target) -> Result<(), PrinterError> {
        let buf = match target {
            Target::Main => self.speculation.pop().ok_or(PrinterError::NoSpeculation)?,
            Target::Slot(SlotId(id)) => {
                let slot = &mut self.slots[id];
                if slot.dead {
                    return Err(PrinterError::DeadSlot);
                }
                slot.speculation.pop().ok_or(PrinterError::NoSpeculation)?
            }
        };
        self.kill_splices(&buf);
        Ok(())
    }

    /// Close the outstanding deferred session: every slot of the session
    /// dies, and layout errors in the spliced content surface here.
    pub fn resolve(&mut self) -> Result<(), PrinterError> {
        if !self.speculation.is_empty() {
            return Err(PrinterError::OpenSpeculation);
        }
        if !self.pending_resolve {
            return Err(PrinterError::NothingToResolve);
        }
        let mut work = splice_ids(&self.main);
        let mut reachable = Vec::new();
        while let Some(id) = work.pop() {
            if self.slots[id].dead {
                continue;
            }
            reachable.push(id);
            work.extend(splice_ids(&self.slots[id].commands));
        }
        if reachable
            .iter()
            .any(|&id| !self.slots[id].speculation.is_empty())
        {
            return Err(PrinterError::OpenSpeculation);
        }
        for &id in &reachable {
            self.slots[id].dead = true;
        }
        self.pending_resolve = false;
        self.render()?;
        Ok(())
    }

    /// Lay out the document and return everything printed so far.
    pub fn value(&mut self) -> Result<&str, PrinterError> {
        if !self.speculation.is_empty() {
            return Err(PrinterError::OpenSpeculation);
        }
        if self.pending_resolve {
            return Err(PrinterError::UnresolvedDeferred);
        }
        self.rendered = self.render()?;
        Ok(&self.rendered)
    }

    /// Whether `slot` can still be written to.
    pub fn slot_is_live(&self, slot: SlotId) -> bool {
        !self.slots[slot.0].dead
    }

    /// Append a note to the main output: each `\n`-separated line of `text`
    /// is emitted as literal text followed by a hard break, so a note always
    /// occupies whole lines and keeps width accounting correct even when the
    /// note contains newlines.
    pub fn note(&mut self, text: &str) {
        for segment in text.split('\n') {
            if !segment.is_empty() {
                self.dispatch(Target::Main, Cmd::Text(segment.to_string()))
                    .unwrap();
            }
            self.dispatch(Target::Main, Cmd::HardBreak).unwrap();
        }
    }

    fn dispatch(&mut self, target: Target, cmd: Cmd) -> Result<(), PrinterError> {
        match target {
            Target::Main => {
                if let Some(buf) = self.speculation.last_mut() {
                    buf.push(cmd);
                    return Ok(());
                }
                match &cmd {
                    Cmd::BeginGroup { .. } => self.open_groups += 1,
                    Cmd::EndGroup { .. } => {
                        if self.slots.is_empty() && self.open_groups == 0 {
                            return Err(PrinterError::UnbalancedGroup);
                        }
                        self.open_groups -= 1;
                    }
                    Cmd::Splice(_) => self.pending_resolve = true,
                    _ => {}
                }
                self.main.push(cmd);
                Ok(())
            }
            Target::Slot(SlotId(id)) => {
                let slot = &mut self.slots[id];
                if slot.dead {
                    return Err(PrinterError::DeadSlot);
                }
                if let Some(buf) = slot.speculation.last_mut() {
                    buf.push(cmd);
                } else {
                    slot.commands.push(cmd);
                }
                Ok(())
            }
        }
    }

    fn kill_splices(&mut self, cmds: &[Cmd]) {
        let mut work = splice_ids(cmds);
        while let Some(id) = work.pop() {
            let slot = &mut self.slots[id];
            slot.dead = true;
            work.extend(splice_ids(&slot.commands));
            for buf in &slot.speculation {
                work.extend(splice_ids(buf));
            }
        }
    }

    fn render(&self) -> Result<String, PrinterError> {
        let mut force = vec![false];
        let mut stack = Vec::new();
        mark_comment_groups(&self.slots, &self.main, &mut force, &mut stack);
        let mut renderer = Renderer::new(self.max_width, &self.slots, force);
        renderer.run(&self.main)?;
        Ok(renderer.finish())
    }
}

struct Renderer<'a> {
    max_width: usize,
    slots: &'a [Slot],
    force_break: Vec<bool>,
    out: String,
    output_width: usize,
    at_line_start: bool,
    buffer: VecDeque<Token>,
    buffer_width: usize,
    pending_comments: String,
    indentation: isize,
    groups: Vec<Group>,
    group_stack: Vec<usize>,
    group_queue: Vec<Vec<usize>>,
}

impl<'a> Renderer<'a> {
    fn new(max_width: usize, slots: &'a [Slot], force_break: Vec<bool>) -> Renderer<'a> {
        Renderer {
            max_width,
            slots,
            force_break,
            out: String::new(),
            output_width: 0,
            at_line_start: true,
            buffer: VecDeque::new(),
            buffer_width: 0,
            pending_comments: String::new(),
            indentation: 0,
            groups: vec![Group {
                depth: 0,
                indent: 0,
                pending: 0,
                want_break: false,
                comment: false,
            }],
            group_stack: vec![0],
            group_queue: vec![vec![0]],
        }
    }

    fn run(&mut self, cmds: &'a [Cmd]) -> Result<(), PrinterError> {
        for cmd in cmds {
            self.execute(cmd)?;
        }
        Ok(())
    }

    fn execute(&mut self, cmd: &'a Cmd) -> Result<(), PrinterError> {
        match cmd {
            Cmd::Text(s) => self.text(s),
            Cmd::Breakable(sep) => self.breakable(sep),
            Cmd::IfBreak(s) => self.if_break(s),
            Cmd::HardBreak => self.newline(),
            Cmd::BeginGroup { indent, open } => self.begin_group(*indent, open),
            Cmd::EndGroup { close } => return self.end_group(close),
            Cmd::ShiftIndent(delta) => self.indentation += delta,
            Cmd::Comment(s) => self.comment(s),
            Cmd::Splice(SlotId(id)) => {
                let spliced = self.slots[*id].commands.as_slice();
                return self.run(spliced);
            }
        }
        Ok(())
    }

    fn finish(mut self) -> String {
        self.flush();
        let pending = std::mem::take(&mut self.pending_comments);
        self.out.push_str(&pending);
        self.out
    }

    fn text(&mut self, s: &str) {
        let width = s.chars().count();
        if self.buffer.is_empty() {
            self.out.push_str(s);
            self.output_width += width;
            if width > 0 {
                self.at_line_start = false;
            }
        } else {
            match self.buffer.back_mut() {
                Some(Token::Text { content, width: w }) => {
                    content.push_str(s);
                    *w += width;
                }
                _ => self.buffer.push_back(Token::Text {
                    content: s.to_string(),
                    width,
                }),
            }
            self.buffer_width += width;
            self.break_outer_groups();
        }
    }

    fn breakable(&mut self, sep: &str) {
        let group = *self.group_stack.last().unwrap();
        if self.groups[group].want_break {
            self.newline();
        } else {
            let width = sep.chars().count();
            self.buffer.push_back(Token::Breakable {
                sep: sep.to_string(),
                width,
                indent: self.indentation,
                group,
            });
            self.groups[group].pending += 1;
            self.buffer_width += width;
            self.break_outer_groups();
        }
    }

    fn if_break(&mut self, s: &str) {
        let group = *self.group_stack.last().unwrap();
        if self.groups[group].want_break {
            self.text(s);
        } else {
            self.buffer.push_back(Token::IfBreak {
                content: s.to_string(),
                group,
            });
        }
    }

    fn comment(&mut self, s: &str) {
        if self.buffer.is_empty() {
            self.pending_comments.push_str(s);
        } else {
            self.buffer.push_back(Token::Comment {
                content: s.to_string(),
            });
        }
    }

    fn emit_pending_comments(&mut self) {
        let pending = std::mem::take(&mut self.pending_comments);
        self.out.push_str(&pending);
    }

    fn newline(&mut self) {
        self.flush();
        self.emit_pending_comments();
        self.out.push('\n');
        self.out.push_str(&spaces(self.indentation));
        self.output_width = self.indentation.max(0) as usize;
        self.at_line_start = true;
    }

    fn begin_group(&mut self, indent: usize, open: &str) {
        if !open.is_empty() {
            self.text(open);
        }
        let depth = self.groups[*self.group_stack.last().unwrap()].depth + 1;
        let id = self.groups.len();
        let comment = self.force_break[id];
        self.groups.push(Group {
            depth,
            indent,
            pending: 0,
            want_break: comment,
            comment,
        });
        self.group_stack.push(id);
        if self.group_queue.len() <= depth {
            self.group_queue.resize_with(depth + 1, Vec::new);
        }
        self.group_queue[depth].push(id);
        self.indentation += indent as isize;
    }

    fn end_group(&mut self, close: &str) -> Result<(), PrinterError> {
        if self.group_stack.len() == 1 {
            return Err(PrinterError::UnbalancedGroup);
        }
        let id = self.group_stack.pop().unwrap();
        self.indentation -= self.groups[id].indent as isize;
        let close = if self.groups[id].comment {
            // A breakable the client placed before the close has already
            // started the fresh line; add the close's own break only when
            // the close would otherwise share a line with content.
            if !(self.buffer.is_empty() && self.at_line_start) {
                self.newline();
            }
            close.trim_start()
        } else {
            close
        };
        if self.groups[id].pending == 0 {
            self.queue_remove(id);
        }
        if !close.is_empty() {
            self.text(close);
        }
        Ok(())
    }

    fn queue_remove(&mut self, id: usize) {
        let level = &mut self.group_queue[self.groups[id].depth];
        if let Some(pos) = level.iter().position(|&group| group == id) {
            level.remove(pos);
        }
    }

    fn break_outer_groups(&mut self) {
        while self.output_width + self.buffer_width > self.max_width {
            let Some(id) = self.deq() else {
                return;
            };
            while self.groups[id].pending > 0 {
                let token = self.buffer.pop_front().unwrap();
                self.buffer_width -= token.width();
                self.output_token(token);
            }
            while matches!(
                self.buffer.front(),
                Some(Token::Text { .. } | Token::IfBreak { .. } | Token::Comment { .. })
            ) {
                let token = self.buffer.pop_front().unwrap();
                self.buffer_width -= token.width();
                self.output_token(token);
            }
        }
    }

    fn deq(&mut self) -> Option<usize> {
        for level in self.group_queue.iter_mut() {
            if let Some(pos) = level
                .iter()
                .position(|&group| self.groups[group].pending > 0)
            {
                let id = level.remove(pos);
                self.groups[id].want_break = true;
                return Some(id);
            }
            for &group in level.iter() {
                self.groups[group].want_break = true;
            }
            level.clear();
        }
        None
    }

    fn output_token(&mut self, token: Token) {
        match token {
            Token::Text { content, width } => {
                self.out.push_str(&content);
                self.output_width += width;
                if width > 0 {
                    self.at_line_start = false;
                }
            }
            Token::IfBreak { content, group } => {
                if self.groups[group].want_break {
                    let width = content.chars().count();
                    self.out.push_str(&content);
                    self.output_width += width;
                    if width > 0 {
                        self.at_line_start = false;
                    }
                }
            }
            Token::Comment { content } => {
                self.pending_comments.push_str(&content);
            }
            Token::Breakable {
                sep,
                width,
                indent,
                group,
            } => {
                self.groups[group].pending -= 1;
                if self.groups[group].want_break {
                    self.emit_pending_comments();
                    self.out.push('\n');
                    self.out.push_str(&spaces(indent));
                    self.output_width = indent.max(0) as usize;
                    self.at_line_start = true;
                } else {
                    if self.groups[group].pending == 0 {
                        self.queue_remove(group);
                    }
                    self.out.push_str(&sep);
                    self.output_width += width;
                    if width > 0 {
                        self.at_line_start = false;
                    }
                }
            }
        }
    }

    fn flush(&mut self) {
        while let Some(token) = self.buffer.pop_front() {
            self.output_token(token);
        }
        self.buffer_width = 0;
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/printer_tests.rs"]
mod tests;
