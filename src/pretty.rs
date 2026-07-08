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
    handle: PrinterHandle,
}

impl PrettyPrinter {
    /// Create a printer that keeps lines within `max_width` characters where
    /// the group structure allows it.
    pub fn new(max_width: usize) -> Self {
        PrettyPrinter {
            handle: PrinterHandle::new(max_width as u64),
        }
    }

    /// Emit literal, unbreakable text.
    ///
    /// Newlines in `s` are honored as unconditional line breaks (equivalent
    /// to [`hard_break`](PrettyPrinter::hard_break), so the new line starts
    /// at the current indentation).
    pub fn text(&mut self, s: &str) {
        let mut first = true;
        for segment in s.split('\n') {
            if !first {
                self.hard_break();
            }
            first = false;
            if !segment.is_empty() {
                self.handle.text(segment).unwrap();
            }
        }
    }

    /// Emit a potential break point: renders as `sep` if the enclosing group
    /// fits on the current line, and as a newline plus the current
    /// indentation if the group breaks.
    pub fn breakable(&mut self, sep: &str) {
        self.handle.breakable(sep).unwrap();
    }

    /// Emit an unconditional newline followed by the current indentation.
    pub fn hard_break(&mut self) {
        self.handle.hard_break().unwrap();
    }

    /// Open a group: emit `open`, then increase the indentation applied by
    /// subsequent break points by `indent` (conventionally the width of
    /// `open`, so continuation lines align just inside the delimiter).
    pub fn begin_group(&mut self, indent: usize, open: &str) {
        self.handle.begin_group(indent as u64, open).unwrap();
    }

    /// Close the innermost group: decrease the indentation by `dedent`, then
    /// emit `close`. Panics if no group is open.
    pub fn end_group(&mut self, dedent: usize, close: &str) {
        self.handle.end_group(dedent as u64, close).unwrap();
    }

    /// Adjust the indentation applied by subsequent break points by `delta`.
    pub fn shift_indent(&mut self, delta: isize) {
        self.handle.shift_indent(delta as i64).unwrap();
    }

    /// Flush pending break points and return everything printed so far.
    pub fn value(&mut self) -> String {
        self.handle.value().unwrap()
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

/// Implement [`PrettyPrintable`] for one or more `Debug` types by printing
/// their `{:?}` representation.
///
/// This is the escape hatch for types whose `Debug` output is already the
/// representation you want (or whose definition you do not control, so
/// `#[derive(PrettyPrintable)]` is unavailable). Any newlines in the `Debug`
/// output become line breaks at the current indentation; no further layout
/// is applied.
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
                printer.text(&format!("{:?}", self));
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
