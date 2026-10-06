use hegel::{Document, PrettyPrinter};

use std::collections::BTreeMap;
use std::fmt::Debug;

static_assertions::assert_not_impl_any!(PrettyPrinter: Sync);

fn render<T: Debug + ?Sized>(value: &T, max_width: usize) -> String {
    let mut doc = Document::new().max_width(max_width);
    doc.printer().debug(value);
    doc.finish()
}

#[test]
fn debug_prints_flat_values_as_their_debug_output() {
    assert_eq!(render(&42i32, 79), "42");
    assert_eq!(render(&true, 79), "true");
    assert_eq!(render(&'\n', 79), "'\\n'");
    assert_eq!(render("hi", 79), "\"hi\"");
    assert_eq!(render(&String::from("a\nb"), 79), "\"a\\nb\"");
    assert_eq!(render(&1.5f64, 79), "1.5");
    assert_eq!(render(&f64::NAN, 79), "NaN");
    assert_eq!(render(&(1, "a"), 79), "(1, \"a\")");
    assert_eq!(render(&vec![1, 2, 3], 79), "[1, 2, 3]");
    assert_eq!(render(&Some(Some("x")), 79), "Some(Some(\"x\"))");
    let map: BTreeMap<i32, &str> = [(1, "a"), (2, "b")].into_iter().collect();
    assert_eq!(render(&map, 79), "{1: \"a\", 2: \"b\"}");
}

#[test]
fn debug_output_breaks_one_item_per_line_when_it_overflows() {
    assert_eq!(render(&vec![1, 2, 3], 6), "[1,\n 2,\n 3]");
    assert_eq!(render(&(111, 222, 333), 8), "(111,\n 222,\n 333)");
    assert_eq!(
        render(&vec![vec![1, 2], vec![3, 4]], 12),
        "[[1, 2],\n [3, 4]]"
    );
    let map: BTreeMap<i32, &str> = [(1, "a"), (2, "b")].into_iter().collect();
    assert_eq!(render(&map, 12), "{1: \"a\",\n 2: \"b\"}");
    assert_eq!(
        render(&Box::new(vec!["aaaa"; 3]), 12),
        "[\"aaaa\",\n \"aaaa\",\n \"aaaa\"]"
    );
}

#[derive(Debug)]
#[allow(dead_code)]
struct Nested {
    name: &'static str,
    values: [i32; 5],
    pair: (bool, char),
}

#[test]
fn derived_debug_structs_break_in_block_style() {
    let value = Nested {
        name: "abcdef",
        values: [100, 200, 300, 400, 500],
        pair: (true, 'x'),
    };
    assert_eq!(
        render(&value, 79),
        "Nested { name: \"abcdef\", values: [100, 200, 300, 400, 500], pair: (true, 'x') }"
    );
    assert_eq!(
        render(&value, 30),
        "Nested {\n    name: \"abcdef\",\n    values: [100,\n             200,\n             300,\n             400,\n             500],\n    pair: (true, 'x') }"
    );
}

struct MultiLineDebug;

impl Debug for MultiLineDebug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line one\nline two")
    }
}

#[test]
fn hand_written_debug_output_is_emitted_verbatim_at_the_current_indentation() {
    assert_eq!(render(&MultiLineDebug, 79), "line one\nline two");

    let mut doc = Document::new();
    let printer = doc.printer();
    printer.shift_indent(2);
    printer.debug(&MultiLineDebug);
    assert_eq!(doc.finish(), "line one\n  line two");
}

#[test]
fn reflow_lays_out_a_preformatted_representation() {
    let mut doc = Document::new().max_width(12);
    doc.printer().reflow("Point { x: 100, y: 200 }");
    assert_eq!(doc.finish(), "Point {\n    x: 100,\n    y: 200 }");

    let mut doc = Document::new().max_width(6);
    doc.printer().reflow("unbalanced [100, 200");
    assert_eq!(doc.finish(), "unbalanced [100, 200");
}

struct FormatsOnlyWhenPrinting;

impl Debug for FormatsOnlyWhenPrinting {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("formatted for a printer that discards its output");
    }
}

#[test]
fn debug_does_not_format_for_a_printer_that_is_not_printing() {
    PrettyPrinter::noop().debug(&FormatsOnlyWhenPrinting);
    let mut doc = Document::new();
    let mut child = doc.printer().clone();
    doc.finish();
    child.debug(&FormatsOnlyWhenPrinting);
}

#[test]
fn group_seq_and_separator_compose_a_layout() {
    let mut doc = Document::new().max_width(79);
    doc.printer().group(6, "Entry(", ")", |p| {
        p.debug(&1);
        p.separator();
        p.seq("[", "]", ["a", "b"], |p, item| p.text(item));
    });
    assert_eq!(doc.finish(), "Entry(1, [a, b])");

    let mut doc = Document::new().max_width(12);
    doc.printer().group(6, "Entry(", ")", |p| {
        p.debug(&1);
        p.separator();
        p.seq("[", "]", ["aaaa", "bbbb"], |p, item| p.text(item));
    });
    assert_eq!(doc.finish(), "Entry(1,\n      [aaaa,\n       bbbb])");

    let mut doc = Document::new();
    doc.printer()
        .seq("{", "}", Vec::<i32>::new(), |p, n| p.debug(&n));
    assert_eq!(doc.finish(), "{}");
}

#[test]
fn printer_text_treats_newlines_as_hard_breaks() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.shift_indent(4);
    printer.text("a\nb");
    printer.shift_indent(-4);
    printer.hard_break();
    printer.text("c");
    assert_eq!(doc.finish(), "a\n    b\nc");
}

#[test]
fn printer_groups_lay_out_inline_or_broken() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.begin_group(1, "[");
    printer.text("1,");
    printer.breakable(" ");
    printer.text("2");
    printer.end_group("]");
    assert_eq!(doc.finish(), "[1, 2]");
}

#[test]
fn deferred_holes_fill_in_before_rendering() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.text("a");
    let mut slot = printer.clone();
    printer.text("d");
    slot.text("b\nc");
    assert_eq!(doc.finish(), "ab\ncd");
}

#[test]
fn deferred_slots_outliving_their_document_ignore_writes() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.text("a");
    let mut slot = printer.clone();
    slot.text("b");
    assert_eq!(doc.finish(), "ab");
    slot.text("ignored");
    slot.breakable(" ");
    slot.reflow("[1, 2]");
}

#[test]
fn comments_attach_to_line_ends_and_break_open_groups() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.begin_group(1, "[");
    printer.text("1,");
    printer.breakable(" ");
    printer.text("2");
    printer.comment("or any other generated value");
    printer.text(",");
    printer.breakable(" ");
    printer.text("3");
    printer.end_group("]");
    assert_eq!(
        doc.finish(),
        "[1,\n 2,  // or any other generated value\n 3\n]"
    );
}

#[test]
fn comments_outside_groups_do_not_affect_layout() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.text("let x = 0;");
    printer.comment("or any other generated value");
    printer.hard_break();
    printer.text("let y = 1;");
    assert_eq!(
        doc.finish(),
        "let x = 0;  // or any other generated value\nlet y = 1;"
    );
}

#[test]
#[should_panic(expected = "must not contain newlines")]
fn comments_with_newlines_panic() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.comment("a\nb");
}

#[test]
fn printer_debug_form_is_opaque() {
    let mut doc = Document::new();
    let printer = doc.printer();
    assert_eq!(
        format!("{printer:?}"),
        "PrettyPrinter { handle: Some(PrinterHandle { .. }) }"
    );
}

#[test]
#[should_panic(expected = "matching begin_group")]
fn unbalanced_end_group_panics() {
    let mut doc = Document::new();
    let printer = doc.printer();
    printer.end_group("]");
}

#[test]
#[should_panic(expected = "max_width must be positive")]
fn zero_width_printer_panics() {
    Document::new().max_width(0);
}

#[test]
fn end_group_dedents_by_the_full_open_delimiter_width() {
    let mut doc = Document::new().max_width(12);
    let printer = doc.printer();
    printer.begin_group(5, "Some(");
    printer.begin_group(1, "[");
    printer.text("first,");
    printer.breakable(" ");
    printer.text("second");
    printer.end_group("]");
    printer.end_group(")");
    printer.hard_break();
    printer.text("x");
    assert_eq!(doc.finish(), "Some([first,\n      second])\nx");
}

#[test]
fn should_print_distinguishes_real_and_noop_printers() {
    assert!(Document::new().printer().should_print());
    assert!(!PrettyPrinter::noop().should_print());
}

#[test]
fn noop_printer_discards_everything() {
    let mut printer = PrettyPrinter::noop();
    printer.begin_group(1, "[");
    printer.text("first");
    printer.text("a\nb");
    printer.breakable(" ");
    printer.hard_break();
    printer.shift_indent(2);
    printer.comment("nothing to see");
    printer.reflow("[1, 2]");
    printer.end_group("]");
    let mut slot = printer.clone();
    slot.text("later\ntext");
    slot.breakable(" ");
    assert!(!printer.should_print());
}

#[test]
fn noop_printer_speculation_commits_aborts_and_drops() {
    let mut printer = PrettyPrinter::noop();
    let mut speculation = printer.speculate();
    speculation.printer().text("kept");
    speculation.commit();
    let mut speculation = printer.speculate();
    speculation.printer().text("discarded");
    speculation.abort();
    {
        let mut speculation = printer.speculate();
        speculation.printer().text("dropped");
    }
    assert!(!printer.should_print());
}

#[test]
fn empty_documents_finish_to_the_empty_string() {
    assert_eq!(Document::new().finish(), "");
    assert_eq!(Document::default().finish(), "");
}

#[test]
fn documents_default_to_a_width_of_79() {
    for (element_width, expected_break) in [(74, false), (75, true)] {
        let mut doc = Document::new();
        let printer = doc.printer();
        printer.begin_group(1, "[");
        printer.text(&"a".repeat(element_width));
        printer.text(",");
        printer.breakable(" ");
        printer.text("b");
        printer.end_group("]");
        assert_eq!(doc.finish().contains('\n'), expected_break);
    }
}

#[test]
#[should_panic(expected = "max_width must be set before the document is printed to")]
fn setting_the_width_after_printing_panics() {
    let mut doc = Document::new();
    doc.printer().text("a");
    doc.max_width(40);
}

#[test]
fn printers_move_between_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<PrettyPrinter>();
    assert_send::<Document>();

    let mut doc = Document::new();
    doc.printer().text("a");
    let mut child = doc.printer().clone();
    doc.printer().text("c");
    std::thread::spawn(move || {
        child.text("b");
    })
    .join()
    .unwrap();
    assert_eq!(doc.finish(), "abc");
}

#[test]
fn a_clone_stops_printing_once_its_document_is_read() {
    let mut doc = Document::new();
    let mut child = doc.printer().clone();
    assert!(child.should_print());
    doc.finish();
    assert!(!child.should_print());
    child.text("ignored");
}

#[test]
fn cloning_a_dead_region_yields_a_noop_printer() {
    let mut doc = Document::new();
    let child = doc.printer().clone();
    doc.finish();
    let mut grandchild = child.clone();
    assert!(!grandchild.should_print());
    grandchild.text("ignored");
}

#[test]
#[should_panic(expected = "matching begin_group")]
fn unbalanced_groups_in_a_child_region_panic_at_finish() {
    let mut doc = Document::new();
    let mut child = doc.printer().clone();
    child.end_group(")");
    doc.finish();
}
