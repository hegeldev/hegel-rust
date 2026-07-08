//! A pretty-printer with deferred and speculative regions.
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
//! On top of the core sits a recording layer, ported from the deferred
//! printing work in Hypothesis (`RepresentationPrinter.deferred`/`resolve`).
//! [`Printer::deferred`] opens a hole in the output and returns a [`SlotId`];
//! from that point the printer records primitive commands instead of
//! executing them, so content written to the slot later — while the test body
//! runs — comes out at the hole's position when [`Printer::resolve`] replays
//! the recording. Because replay executes real primitives over the complete
//! content, line-breaking behaves exactly as if everything had been printed
//! inline. [`Printer::begin_speculative`] reuses the same machinery for
//! output that may be retracted: commands buffer until
//! [`Printer::commit_speculative`] or [`Printer::abort_speculative`], which
//! is how draw-time printing survives rejection (filters, collection
//! rejection, failed assumptions).
//!
//! Text passed to [`Printer::text`] must not contain newlines; use
//! [`Printer::hard_break`] instead so column and indentation accounting stay
//! correct. Widths are counted in `char`s.

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
/// written to the slot is spliced in at the hole's position on
/// [`Printer::resolve`], after which the slot is dead and all further writes
/// error.
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
    HardBreak,
    BeginGroup { indent: usize, open: String },
    EndGroup { dedent: usize, close: String },
    ShiftIndent(isize),
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
}

impl Token {
    fn width(&self) -> usize {
        match self {
            Token::Text { width, .. } | Token::Breakable { width, .. } => *width,
        }
    }
}

#[derive(Debug)]
struct Group {
    depth: usize,
    pending: usize,
    want_break: bool,
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

/// The pretty printer. See the module docs for the printing model.
#[derive(Debug)]
pub struct Printer {
    max_width: usize,
    out: String,
    output_width: usize,
    buffer: VecDeque<Token>,
    buffer_width: usize,
    indentation: isize,
    groups: Vec<Group>,
    group_stack: Vec<usize>,
    group_queue: Vec<Vec<usize>>,
    recording: Option<Vec<Cmd>>,
    speculation: Vec<Vec<Cmd>>,
    slots: Vec<Slot>,
}

impl Printer {
    /// Create a printer that tries to keep lines within `max_width` chars.
    pub fn new(max_width: usize) -> Printer {
        Printer {
            max_width,
            out: String::new(),
            output_width: 0,
            buffer: VecDeque::new(),
            buffer_width: 0,
            indentation: 0,
            groups: vec![Group {
                depth: 0,
                pending: 0,
                want_break: false,
            }],
            group_stack: vec![0],
            group_queue: vec![vec![0]],
            recording: None,
            speculation: Vec::new(),
            slots: Vec::new(),
        }
    }

    /// Emit literal, unbreakable text. Must not contain newlines.
    pub fn text(&mut self, target: Target, s: &str) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::Text(s.to_string()))
    }

    /// Emit a potential break point: renders as `sep` if the enclosing group
    /// fits on the line, and as a newline plus the current indentation if the
    /// group breaks.
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

    /// Close the innermost group: decrease the indentation by `dedent`, then
    /// emit `close`.
    pub fn end_group(
        &mut self,
        target: Target,
        dedent: usize,
        close: &str,
    ) -> Result<(), PrinterError> {
        self.dispatch(
            target,
            Cmd::EndGroup {
                dedent,
                close: close.to_string(),
            },
        )
    }

    /// Adjust the indentation applied by subsequent break points by `delta`.
    pub fn shift_indent(&mut self, target: Target, delta: isize) -> Result<(), PrinterError> {
        self.dispatch(target, Cmd::ShiftIndent(delta))
    }

    /// Open a deferred hole at the current position of `target` and return
    /// its slot. Writes to the slot are spliced in at the hole's position
    /// when [`Printer::resolve`] runs.
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
    /// content.
    pub fn commit_speculative(&mut self, target: Target) -> Result<(), PrinterError> {
        match target {
            Target::Main => {
                let buf = self.speculation.pop().ok_or(PrinterError::NoSpeculation)?;
                if let Some(outer) = self.speculation.last_mut() {
                    outer.extend(buf);
                } else if let Some(recording) = self.recording.as_mut() {
                    recording.extend(buf);
                } else {
                    for cmd in buf {
                        self.dispatch(Target::Main, cmd)?;
                    }
                }
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

    /// Replay the outstanding deferred session: every hole's content is
    /// spliced in at its position, line-breaking decisions are made over the
    /// complete output, and every slot of the session dies.
    pub fn resolve(&mut self) -> Result<(), PrinterError> {
        if !self.speculation.is_empty() {
            return Err(PrinterError::OpenSpeculation);
        }
        let recording = self
            .recording
            .take()
            .ok_or(PrinterError::NothingToResolve)?;
        self.replay(recording)
    }

    /// Flush pending break points and return everything printed so far.
    pub fn value(&mut self) -> Result<&str, PrinterError> {
        if !self.speculation.is_empty() {
            return Err(PrinterError::OpenSpeculation);
        }
        if self.recording.is_some() {
            return Err(PrinterError::UnresolvedDeferred);
        }
        self.flush();
        Ok(&self.out)
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
                    Ok(())
                } else if let Some(recording) = self.recording.as_mut() {
                    recording.push(cmd);
                    Ok(())
                } else {
                    self.execute(cmd)
                }
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

    fn execute(&mut self, cmd: Cmd) -> Result<(), PrinterError> {
        match cmd {
            Cmd::Text(s) => self.text_live(&s),
            Cmd::Breakable(sep) => self.breakable_live(sep),
            Cmd::HardBreak => self.hard_break_live(),
            Cmd::BeginGroup { indent, open } => self.begin_group_live(indent, &open),
            Cmd::EndGroup { dedent, close } => return self.end_group_live(dedent, &close),
            Cmd::ShiftIndent(delta) => self.indentation += delta,
            Cmd::Splice(slot) => self.recording = Some(vec![Cmd::Splice(slot)]),
        }
        Ok(())
    }

    fn replay(&mut self, cmds: Vec<Cmd>) -> Result<(), PrinterError> {
        for cmd in cmds {
            match cmd {
                Cmd::Splice(SlotId(id)) => {
                    let slot = &mut self.slots[id];
                    if !slot.speculation.is_empty() {
                        return Err(PrinterError::OpenSpeculation);
                    }
                    slot.dead = true;
                    let inner = std::mem::take(&mut slot.commands);
                    self.replay(inner)?;
                }
                other => self.execute(other)?,
            }
        }
        Ok(())
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

    fn text_live(&mut self, s: &str) {
        let width = s.chars().count();
        if self.buffer.is_empty() {
            self.out.push_str(s);
            self.output_width += width;
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

    fn breakable_live(&mut self, sep: String) {
        let group = *self.group_stack.last().unwrap();
        if self.groups[group].want_break {
            self.newline();
        } else {
            let width = sep.chars().count();
            self.buffer.push_back(Token::Breakable {
                sep,
                width,
                indent: self.indentation,
                group,
            });
            self.groups[group].pending += 1;
            self.buffer_width += width;
            self.break_outer_groups();
        }
    }

    fn hard_break_live(&mut self) {
        self.newline();
    }

    fn newline(&mut self) {
        self.flush();
        self.out.push('\n');
        self.out.push_str(&spaces(self.indentation));
        self.output_width = self.indentation.max(0) as usize;
    }

    fn begin_group_live(&mut self, indent: usize, open: &str) {
        if !open.is_empty() {
            self.text_live(open);
        }
        let depth = self.groups[*self.group_stack.last().unwrap()].depth + 1;
        let id = self.groups.len();
        self.groups.push(Group {
            depth,
            pending: 0,
            want_break: false,
        });
        self.group_stack.push(id);
        if self.group_queue.len() <= depth {
            self.group_queue.resize_with(depth + 1, Vec::new);
        }
        self.group_queue[depth].push(id);
        self.indentation += indent as isize;
    }

    fn end_group_live(&mut self, dedent: usize, close: &str) -> Result<(), PrinterError> {
        if self.group_stack.len() == 1 {
            return Err(PrinterError::UnbalancedGroup);
        }
        self.indentation -= dedent as isize;
        let id = self.group_stack.pop().unwrap();
        if self.groups[id].pending == 0 {
            self.queue_remove(id);
        }
        if !close.is_empty() {
            self.text_live(close);
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
            while matches!(self.buffer.front(), Some(Token::Text { .. })) {
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
            }
            Token::Breakable {
                sep,
                width,
                indent,
                group,
            } => {
                self.groups[group].pending -= 1;
                if self.groups[group].want_break {
                    self.out.push('\n');
                    self.out.push_str(&spaces(indent));
                    self.output_width = indent.max(0) as usize;
                } else {
                    if self.groups[group].pending == 0 {
                        self.queue_remove(group);
                    }
                    self.out.push_str(&sep);
                    self.output_width += width;
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
