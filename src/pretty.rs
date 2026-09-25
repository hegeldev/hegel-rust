//! Pretty-printing of generated values: how a failing test reports what it
//! drew, and how to take part in that report.
//!
//! When a test fails, Hegel replays the minimal failing example and prints
//! every drawn value as a `let` binding:
//!
//! ```text
//! let records = [Record {
//!          name: "000",
//!          tags: None,
//!          scores: {"0": 0} }];
//! ```
//!
//! Values print the way `{:?}` shows them — the `Debug` representation every
//! Rust type already has — laid out by libhegel's layout engine so that a
//! large value wraps one element or field per line instead of running off
//! the edge of the terminal. This page explains the machinery behind that
//! report and what to do when the compiler tells you a draw is not
//! printable.
//!
//! # Printing is the generator's job
//!
//! A value often cannot print itself: there is no `Debug` for a drawn
//! closure, and the useful representation of a `HegelRandom` (the `rand`
//! integration's fake PRNG) is the sequence of values it hands out *after*
//! it is drawn. The process that constructed a value can always describe it,
//! so in Hegel printing is a capability of the generator, not the value.
//! That capability is the
//! [`PrintableGenerator`](crate::PrintableGenerator) trait: a
//! [`Generator`](crate::Generator) draws silently through `do_draw`, and a
//! `PrintableGenerator` can also draw-and-describe through
//! `do_draw_and_print`. [`TestCase::draw`](crate::TestCase::draw) accepts
//! only printable generators, so that the failure report can say what was
//! drawn; [`TestCase::draw_silent`](crate::TestCase::draw_silent) accepts
//! any generator and reports nothing.
//!
//! Most generators are printable without anyone thinking about it:
//!
//! - Every leaf generator prints: integers, floats, booleans, strings and
//!   regexes, bytes, characters, dates and times, UUIDs, IP addresses,
//!   emails, URLs, durations.
//! - Structural combinators print whenever their components do:
//!   collections, tuples, [`optional`](crate::generators::optional),
//!   [`one_of!`](crate::one_of), `filter`, `flat_map`,
//!   [`recursive`](crate::generators::recursive), and the generators
//!   `#[derive(DefaultGenerator)]` produces. These print as they draw,
//!   element by element and field by field, so an element's own printing
//!   (a [`print_with`](crate::Generator::print_with) closure, a deferred
//!   `HegelRandom`) shows up inside the containing value.
//! - Value-producing combinators print whenever the produced type
//!   implements `Debug`: `map`, [`just`](crate::generators::just),
//!   [`sampled_from`](crate::generators::sampled_from),
//!   [`boxed`](crate::Generator::boxed), and
//!   [`#[hegel::composite]`](crate::composite) functions. These print the
//!   finished value through [`PrettyPrinter::debug`], not the draws that
//!   built it: a composite's inner draws never appear in the report, only
//!   its return value.
//!
//! # When a draw is not printable
//!
//! A draw fails to compile only when a generator that prints by value
//! produces a type without a `Debug` implementation, or when a hand-written
//! [`Generator`](crate::Generator) never implemented printing. The fixes,
//! in order of preference:
//!
//! - `#[derive(Debug)]` on your own type makes every generator of it
//!   printable at once.
//! - [`print_with`](crate::Generator::print_with) — print a custom
//!   representation from a closure taking the value and the printer:
//!   `.print_with(|value, printer| printer.text(&format!("make({})", value.id)))`.
//!   Also the way to mask a secret, or to print a foreign type whose `Debug`
//!   output is opaque (a tagged pointer, a bit-packed struct).
//! - [`print_as_debug`](crate::Generator::print_as_debug) — make a
//!   hand-written generator of a `Debug` type printable.
//! - [`print_as_call`](crate::generators::Mapped::print_as_call) — on a
//!   `map` whose input draw is printable, print the input instead of the
//!   output: `.map(KeyData::from_ffi).print_as_call("KeyData::from_ffi")`
//!   reports `KeyData::from_ffi(3)`.
//! - [`TestCase::draw_silent`](crate::TestCase::draw_silent) — draw without
//!   reporting the value, when it isn't worth reporting.
//!
//! # Helpers and type erasure
//!
//! Printability lives in the generator's concrete type, so anything that
//! erases the type can erase printability with it — and the compile error
//! then appears at the draw sites, far from the erasing line. There are two
//! forms to watch for:
//!
//! - A helper declared `-> impl Generator<T>` is only ever a silent
//!   generator to its callers, no matter what it returns — a printing
//!   adapter added inside the helper changes nothing. Declare
//!   generator-returning helpers `-> impl PrintableGenerator<T>`.
//! - [`Generator::boxed`](crate::Generator::boxed) keeps only value
//!   printing: the boxed generator prints drawn values by their `Debug`
//!   representation, so boxing preserves printability for `Debug` value
//!   types but drops a printing strategy carried by the erased generator (a
//!   `print_with` closure, say). To keep the generator's own printing
//!   through the erasure, box with
//!   [`boxed_printable`](crate::PrintableGenerator::boxed_printable)
//!   instead, which exists only on generators that are already printable.
//!
//! The adapter methods come from the [`Generator`](crate::Generator) trait
//! and `boxed_printable` from [`PrintableGenerator`](crate::PrintableGenerator),
//! so both need to be in scope. `use hegel::prelude::*;` imports both (see
//! [`prelude`](crate::prelude)).
//!
//! # Draw names
//!
//! The `let records = …` name above is the binding's own name:
//! `#[hegel::test]` (and `#[state_machine]` rule bodies) rewrite
//! `let x = tc.draw(..)` so the report names the draw `x`. The rewrite only
//! sees draws written directly in that body — a draw inside a helper
//! function falls back to the anonymous `draw_1`, `draw_2`, …. Mark such a
//! helper [`#[hegel::test_helper]`](macro@crate::test_helper) to apply the
//! same rewrite to its body (names gain a per-call counter: `x_1`, `x_2`, …),
//! or name a single draw with
//! [`TestCase::draw_named`](crate::TestCase::draw_named).
//!
//! # The layout engine
//!
//! Everything above renders through a shared layout engine, which you meet
//! directly when writing a [`print_with`](crate::Generator::print_with)
//! closure or a [`PrintableGenerator`](crate::PrintableGenerator)
//! implementation.
//!
//! [`Document`] owns one pretty-printed document: its builder methods
//! choose the layout options, [`Document::printer`] exposes the surface to
//! write through, and [`Document::finish`] consumes it to render exactly
//! once at the end. In a test run the document belongs to the test case,
//! and user code only ever writes. Rendering happens after the test body
//! finishes.
//!
//! [`PrettyPrinter`] is that write surface, wrapping libhegel's layout
//! engine (an Oppen-style pretty-printer ported from Hypothesis's
//! `hypothesis.vendor.pretty`). Output is built from three primitives:
//! [`PrettyPrinter::text`] emits literal text, [`PrettyPrinter::breakable`]
//! marks a point that renders as a separator when the enclosing group fits
//! on one line and as a newline plus indentation when it does not, and
//! [`PrettyPrinter::begin_group`] / [`PrettyPrinter::end_group`] delimit
//! the groups those decisions are made over. A group either fits — every
//! breakable renders as its separator — or breaks as a whole, outermost
//! groups first. On top of those sit the conveniences most printing code
//! wants: [`PrettyPrinter::debug`] prints any `Debug` value with its
//! bracket structure recovered by libhegel's reflower, so it wraps like a
//! structurally printed one; [`PrettyPrinter::group`] and
//! [`PrettyPrinter::seq`] write a delimited group or a comma-separated
//! sequence from a closure; [`PrettyPrinter::separator`] is the `,` plus
//! break point between two items.
//!
//! Because printing happens *during* the draw, the printer is more than an
//! append-only stream: a combinator that may reject a draw (a `filter`
//! retry, a duplicate collection element) prints each attempt into a
//! speculative region ([`PrettyPrinter::speculate`]) and commits only the
//! accepted one, and cloning a printer opens a child region that later
//! (even from another thread) fills in at the point where the clone was
//! made. Hand-written implementations can use the same mechanisms.

use crate::ffi::{PrinterCallError, PrinterHandle};
use std::cell::Cell;
use std::fmt::Debug;
use std::marker::PhantomData;

/// Accept a printer operation's outcome: misuse panics with libhegel's
/// diagnostic, while writing to a dead region — a straggling thread printing
/// after the document was read, or into a region whose anchor was retracted
/// — is a silent no-op, so a writer that outlives its document never brings
/// the process down.
pub(crate) fn tolerate(result: Result<(), PrinterCallError>) {
    match result {
        Ok(()) | Err(PrinterCallError::DeadRegion) => {}
        Err(PrinterCallError::Other(message)) => panic!("{message}"),
    }
}
use crate::test_case::invalid_argument;

/// The line width documents are laid out to when none is configured.
pub(crate) const DEFAULT_MAX_WIDTH: u64 = 79;

/// One pretty-printed document: the owner of its layout options, its
/// content, and its rendering.
///
/// Configure the layout with the builder methods (before anything is
/// printed), write content through [`printer`](Document::printer), and
/// render by consuming the document with [`finish`](Document::finish) —
/// rendering happens exactly once, at the end. The [`PrettyPrinter`] this
/// hands out is write-only, so code that is *given* a printer (a
/// [`print_with`](crate::Generator::print_with) closure, a
/// [`PrintableGenerator`](crate::PrintableGenerator)) can never render or
/// otherwise observe the document it is contributing to.
///
/// # Example
///
/// ```
/// use hegel::Document;
///
/// let mut doc = Document::new().max_width(10);
/// let p = doc.printer();
/// p.begin_group(1, "[");
/// p.text("first");
/// p.text(",");
/// p.breakable(" ");
/// p.text("second");
/// p.end_group("]");
/// assert_eq!(doc.finish(), "[first,\n second]");
/// ```
#[derive(Debug)]
pub struct Document {
    max_width: u64,
    printer: Option<PrettyPrinter>,
}

impl Document {
    /// Create an empty document with the default layout options (a maximum
    /// line width of 79 characters).
    pub fn new() -> Self {
        Document {
            max_width: DEFAULT_MAX_WIDTH,
            printer: None,
        }
    }

    /// Keep lines within `max_width` characters where the group structure
    /// allows it. Defaults to 79.
    ///
    /// Layout options describe the whole document, so they must be chosen
    /// up front: calling this after [`printer`](Document::printer) has been
    /// used is an error, as is a `max_width` of 0.
    pub fn max_width(mut self, max_width: usize) -> Self {
        if self.printer.is_some() {
            invalid_argument!("max_width must be set before the document is printed to");
        }
        if max_width == 0 {
            invalid_argument!("max_width must be positive");
        }
        self.max_width = max_width as u64;
        self
    }

    /// The printer to write this document's content through.
    pub fn printer(&mut self) -> &mut PrettyPrinter {
        self.printer
            .get_or_insert_with(|| PrettyPrinter::from_handle(PrinterHandle::new(self.max_width)))
    }

    /// Splice any outstanding deferred content into place, lay the document
    /// out, and return it.
    ///
    /// Consuming the document is what makes rendering a once-at-the-end
    /// operation; there is no way to observe a partially built document.
    pub fn finish(mut self) -> String {
        match &mut self.printer {
            Some(printer) => printer.value(),
            None => String::new(),
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

/// The write surface of a pretty-printed document.
///
/// See the [module docs](self) for the printing model. Obtained from
/// [`Document::printer`] — or received, already positioned, by printing
/// code such as a [`print_with`](crate::Generator::print_with) closure.
/// Rejections of the layout protocol (an [`end_group`](PrettyPrinter::end_group)
/// with no open group) panic, since they indicate a bug in the calling
/// printing code.
pub struct PrettyPrinter {
    /// `None` is the no-op printer: every emitting method returns without
    /// doing anything, so one drawing body can serve both the silent and the
    /// printing draw paths.
    handle: Option<PrinterHandle>,
    /// A printer belongs to one thread at a time (it may move — the type is
    /// `Send` — but never be shared), exactly like [`TestCase`]: the region
    /// model makes cross-thread output deterministic only because each
    /// region has a single writer.
    ///
    /// [`TestCase`]: crate::TestCase
    _single_owner: PhantomData<Cell<()>>,
}

impl std::fmt::Debug for PrettyPrinter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrettyPrinter")
            .field("handle", &self.handle)
            .finish()
    }
}

impl PrettyPrinter {
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
        PrettyPrinter {
            handle: None,
            _single_owner: PhantomData,
        }
    }

    /// Whether printing to this printer produces output: `false` for the
    /// discarding printer returned by [`noop`](PrettyPrinter::noop), and for
    /// a printer whose region has died (see [`Clone`](PrettyPrinter#impl-Clone-for-PrettyPrinter)),
    /// whose writes are discarded. Use it to skip work — formatting a value,
    /// say — whose only purpose is to be printed.
    pub fn should_print(&self) -> bool {
        self.handle.as_ref().is_some_and(PrinterHandle::is_live)
    }

    /// Wrap an existing engine printer handle (e.g. a test case's shared
    /// document).
    pub(crate) fn from_handle(handle: PrinterHandle) -> Self {
        PrettyPrinter {
            handle: Some(handle),
            _single_owner: PhantomData,
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
                tolerate(handle.hard_break());
            }
            first = false;
            if !segment.is_empty() {
                tolerate(handle.text(segment));
            }
        }
    }

    /// Print a value's `Debug` representation, laid out through the group
    /// machinery.
    ///
    /// The value is formatted with `{:?}` and handed to libhegel's reflower,
    /// which recovers the bracket structure of the representation — `Name {
    /// field: value, … }`, `Name(…)`, `(…)`, `[…]`, `{key: value, …}`, with
    /// string and character literals kept whole — and re-emits it through
    /// the printer's groups and break points, so a large value wraps one
    /// field or element per line exactly like one printed structurally.
    /// Output that doesn't follow that grammar (a hand-written `Debug`
    /// implementation can produce anything) is emitted verbatim, with
    /// embedded newlines honored as hard breaks. Nothing is formatted when
    /// the printer is not printing.
    ///
    /// This is how every value-printing generator (`map`, `just`, a boxed
    /// generator, a composite) reports its values, and the usual way for a
    /// [`print_with`](crate::Generator::print_with) closure to embed a
    /// component of a larger representation:
    ///
    /// ```
    /// use hegel::Document;
    ///
    /// let mut doc = Document::new().max_width(20);
    /// doc.printer().debug(&vec![(1, "one"), (2, "two")]);
    /// assert_eq!(doc.finish(), "[(1, \"one\"),\n (2, \"two\")]");
    /// ```
    pub fn debug<T: Debug + ?Sized>(&mut self, value: &T) {
        if self.should_print() {
            self.reflow(&format!("{value:?}"));
        }
    }

    /// Lay out an already-formatted flat representation through the group
    /// machinery, as [`debug`](PrettyPrinter::debug) does for a value's
    /// `Debug` output. For a representation produced some other way — a
    /// `Display` implementation, a serialization — that follows the same
    /// bracketed grammar.
    pub fn reflow(&mut self, repr: &str) {
        let Some(handle) = &self.handle else { return };
        tolerate(handle.reflow(repr));
    }

    /// Emit a potential break point: renders as `sep` if the enclosing group
    /// fits on the current line, and as a newline plus the current
    /// indentation if the group breaks.
    pub fn breakable(&mut self, sep: &str) {
        let Some(handle) = &self.handle else { return };
        tolerate(handle.breakable(sep));
    }

    /// Emit an unconditional newline followed by the current indentation.
    pub fn hard_break(&mut self) {
        let Some(handle) = &self.handle else { return };
        tolerate(handle.hard_break());
    }

    /// Open a group: emit `open`, then increase the indentation applied by
    /// subsequent break points by `indent` (conventionally the width of
    /// `open`, so continuation lines align just inside the delimiter).
    pub fn begin_group(&mut self, indent: usize, open: &str) {
        let Some(handle) = &self.handle else { return };
        tolerate(handle.begin_group(indent as u64, open));
    }

    /// Close the innermost group: undo the indentation its
    /// [`begin_group`](PrettyPrinter::begin_group) added, then emit `close`.
    /// Panics if no group is open.
    pub fn end_group(&mut self, close: &str) {
        let Some(handle) = &self.handle else { return };
        tolerate(handle.end_group(close));
    }

    /// Print a group: `open`, then whatever `body` prints with the
    /// indentation of subsequent break points raised by `indent`, then
    /// `close`. The closure form of
    /// [`begin_group`](PrettyPrinter::begin_group) /
    /// [`end_group`](PrettyPrinter::end_group), which cannot be left
    /// unbalanced.
    ///
    /// ```
    /// use hegel::Document;
    ///
    /// let mut doc = Document::new().max_width(12);
    /// doc.printer().group(5, "Some(", ")", |p| {
    ///     p.seq("[", "]", ["first", "second"], |p, item| p.text(item));
    /// });
    /// assert_eq!(doc.finish(), "Some([first,\n      second])");
    /// ```
    pub fn group(&mut self, indent: usize, open: &str, close: &str, body: impl FnOnce(&mut Self)) {
        self.begin_group(indent, open);
        body(self);
        self.end_group(close);
    }

    /// Print `items` as a delimited, comma-separated sequence — `open`, each
    /// item as `print` renders it with a [`separator`](PrettyPrinter::separator)
    /// between consecutive items, `close` — laid out inline when it fits and
    /// one item per line, aligned just inside `open`, when it does not.
    ///
    /// ```
    /// use hegel::Document;
    ///
    /// let mut doc = Document::new().max_width(8);
    /// doc.printer().seq("{", "}", [1, 2, 3], |p, n| p.debug(&n));
    /// assert_eq!(doc.finish(), "{1,\n 2,\n 3}");
    /// ```
    pub fn seq<T>(
        &mut self,
        open: &str,
        close: &str,
        items: impl IntoIterator<Item = T>,
        mut print: impl FnMut(&mut Self, T),
    ) {
        self.group(open.chars().count(), open, close, |printer| {
            for (index, item) in items.into_iter().enumerate() {
                if index > 0 {
                    printer.separator();
                }
                print(printer, item);
            }
        });
    }

    /// Emit the separator between two items of a group: a `,` followed by a
    /// break point that renders as a space when the group fits on one line.
    pub fn separator(&mut self) {
        self.text(",");
        self.breakable(" ");
    }

    /// Adjust the indentation applied by subsequent break points by `delta`.
    pub fn shift_indent(&mut self, delta: isize) {
        let Some(handle) = &self.handle else { return };
        tolerate(handle.shift_indent(delta as i64));
    }

    /// Attach a comment to the line currently being written: `text` is
    /// rendered as `  // text` at the end of that line, every group open at
    /// this position is forced to break — nothing else may share a line with
    /// a comment — and the comment is excluded from line-width accounting. A
    /// group forced to break by a comment also breaks before its closing
    /// delimiter, so the delimiter is not caught up in a comment on the
    /// group's last element.
    ///
    /// `text` must not contain newlines; a comment is a single-line
    /// construct.
    pub fn comment(&mut self, text: &str) {
        let Some(handle) = &self.handle else { return };
        tolerate(handle.comment(&format!("  // {text}")));
    }

    /// Splice in any outstanding deferred content, flush pending break
    /// points, and return everything printed so far. Only ever called by an
    /// owner of the document — [`Document::finish`], or the run lifecycle
    /// reading a test case's document — never by printing code, which only
    /// sees the write surface. Panics on a layout error in the printed
    /// content (an unbalanced `end_group` that could only be detected once
    /// the whole document was assembled).
    pub(crate) fn value(&mut self) -> String {
        self.try_value()
            .unwrap_or_else(|message| panic!("{message}"))
    }

    /// [`value`](PrettyPrinter::value), reporting a layout error in the
    /// printed content as an `Err` instead of panicking — for the run
    /// lifecycle, which renders the output of user printing code after the
    /// test body's panic handling has finished and must not let a printing
    /// bug take down the whole run.
    pub(crate) fn try_value(&mut self) -> Result<String, String> {
        let Some(handle) = &self.handle else {
            unreachable!("only rendering printers have their value read");
        };
        let _ = handle.resolve();
        match handle.value() {
            Ok(rendered) => Ok(rendered),
            Err(PrinterCallError::Other(message)) => Err(message),
            Err(PrinterCallError::DeadRegion) => {
                unreachable!("a document's own region never dies before it renders")
            }
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
            tolerate(handle.begin_speculative());
        }
        Speculation {
            printer: self,
            resolved: false,
        }
    }
}

/// Cloning a printer opens a *child region*: a hole in the document,
/// anchored at the printer's current position, that the clone writes into.
///
/// Whatever the clone prints — at any later point, from any thread that owns
/// it — appears at the anchor when the document renders, with line-breaking
/// behaving as if it had been printed inline. This is how output crosses
/// threads deterministically (each clone's output lands where the clone was
/// made, however the threads were scheduled), and how a generator whose
/// value's representation is only known during test execution (a
/// Hegel-controlled random number generator, say) prints: it clones the
/// printer at draw time and records into the clone as the value is used.
///
/// A child region dies when the document renders, or when a speculative
/// region its anchor sat inside is aborted; a dead region's writes are
/// silent no-ops, so a clone that outlives its document can keep trying to
/// record without consequence. Cloning a no-op printer yields a no-op
/// printer, and cloning into a dead region yields a printer whose writes
/// discard.
impl Clone for PrettyPrinter {
    fn clone(&self) -> Self {
        let handle = match &self.handle {
            None => None,
            Some(handle) => match handle.deferred() {
                Ok(child) => Some(child),
                Err(PrinterCallError::DeadRegion) => None,
                Err(PrinterCallError::Other(message)) => unreachable!("{message}"),
            },
        };
        PrettyPrinter {
            handle,
            _single_owner: PhantomData,
        }
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
            tolerate(handle.commit_speculative());
        }
    }

    /// Close the region, discarding its output.
    pub fn abort(mut self) {
        self.resolved = true;
        if let Some(handle) = &self.printer.handle {
            tolerate(handle.abort_speculative());
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
