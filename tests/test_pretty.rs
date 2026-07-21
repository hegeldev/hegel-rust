use hegel::{PrettyPrintable, PrettyPrinter};

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

fn render<T: PrettyPrintable + ?Sized>(value: &T, max_width: usize) -> String {
    let mut printer = PrettyPrinter::new(max_width);
    value.pretty_print(&mut printer);
    printer.value()
}

#[test]
fn integers_and_bools_print_as_literals() {
    assert_eq!(render(&42i32, 79), "42");
    assert_eq!(render(&-7i64, 79), "-7");
    assert_eq!(render(&255u8, 79), "255");
    assert_eq!(render(&i128::MIN, 79), i128::MIN.to_string());
    assert_eq!(render(&3usize, 79), "3");
    assert_eq!(render(&true, 79), "true");
    assert_eq!(render(&false, 79), "false");
}

#[test]
fn chars_and_strings_print_escaped() {
    assert_eq!(render(&'a', 79), "'a'");
    assert_eq!(render(&'\n', 79), "'\\n'");
    assert_eq!(render("hi", 79), "\"hi\"");
    assert_eq!(render(&"hi", 79), "\"hi\"");
    assert_eq!(render(&String::from("a\nb"), 79), "\"a\\nb\"");
}

#[test]
fn finite_floats_print_as_debug() {
    assert_eq!(render(&1.5f64, 79), "1.5");
    assert_eq!(render(&1.0f64, 79), "1.0");
    assert_eq!(render(&-0.0f64, 79), "-0.0");
    assert_eq!(render(&2.5f32, 79), "2.5");
}

#[test]
fn non_finite_floats_print_as_expressions() {
    assert_eq!(render(&f64::NAN, 79), "f64::NAN");
    assert_eq!(render(&f32::NAN, 79), "f32::NAN");
    assert_eq!(render(&f64::INFINITY, 79), "f64::INFINITY");
    assert_eq!(render(&f64::NEG_INFINITY, 79), "f64::NEG_INFINITY");
    assert_eq!(render(&f32::INFINITY, 79), "f32::INFINITY");
    assert_eq!(render(&f32::NEG_INFINITY, 79), "f32::NEG_INFINITY");
    assert_eq!(
        render(&f64::from_bits(0x7ff8000000000001), 79),
        "f64::from_bits(0x7ff8000000000001)"
    );
    assert_eq!(
        render(&f64::from_bits(0xfff8000000000000), 79),
        "f64::from_bits(0xfff8000000000000)"
    );
    assert_eq!(
        render(&f32::from_bits(0xffc00000), 79),
        "f32::from_bits(0xffc00000)"
    );
}

#[test]
fn tuples_print_with_rust_syntax() {
    assert_eq!(render(&(), 79), "()");
    assert_eq!(render(&(5,), 79), "(5,)");
    assert_eq!(render(&(1, "a"), 79), "(1, \"a\")");
    assert_eq!(render(&(1, 2.5f64, true), 79), "(1, 2.5, true)");
    assert_eq!(
        render(&(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12), 79),
        "(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12)"
    );
}

#[test]
fn wide_tuples_break_one_element_per_line() {
    assert_eq!(render(&(111, 222, 333), 8), "(111,\n 222,\n 333)");
}

#[test]
fn sequences_print_inline_when_they_fit() {
    assert_eq!(render(&vec![1, 2, 3], 79), "vec![1, 2, 3]");
    assert_eq!(render(&Vec::<i32>::new(), 79), "vec![]");
    assert_eq!(render(&[1, 2], 79), "[1, 2]");
    assert_eq!(render(&[1, 2][..], 79), "[1, 2]");
}

#[test]
fn sequences_break_when_they_overflow() {
    assert_eq!(render(&vec![1, 2, 3], 6), "vec![1,\n     2,\n     3]");
    assert_eq!(render(&[1, 2, 3], 6), "[1,\n 2,\n 3]");
    assert_eq!(
        render(&vec![vec![1, 2], vec![3, 4]], 16),
        "vec![vec![1, 2],\n     vec![3, 4]]"
    );
}

#[test]
fn options_and_results_print_as_constructors() {
    assert_eq!(render(&None::<i32>, 79), "None");
    assert_eq!(render(&Some(3), 79), "Some(3)");
    assert_eq!(render(&Some(Some("x")), 79), "Some(Some(\"x\"))");
    assert_eq!(render(&Ok::<i32, bool>(1), 79), "Ok(1)");
    assert_eq!(render(&Err::<i32, &str>("x"), 79), "Err(\"x\")");
}

#[test]
fn maps_and_sets_print_as_from_constructors() {
    let map: BTreeMap<i32, &str> = [(1, "a"), (2, "b")].into_iter().collect();
    assert_eq!(render(&map, 79), "BTreeMap::from([(1, \"a\"), (2, \"b\")])");
    assert_eq!(render(&BTreeMap::<i32, i32>::new(), 79), "BTreeMap::from([])");
    assert_eq!(
        render(&map, 24),
        "BTreeMap::from([(1, \"a\"),\n                (2, \"b\")])"
    );

    let set: BTreeSet<i32> = [1, 2].into_iter().collect();
    assert_eq!(render(&set, 79), "BTreeSet::from([1, 2])");

    let map: HashMap<i32, &str> = [(1, "a")].into_iter().collect();
    assert_eq!(render(&map, 79), "HashMap::from([(1, \"a\")])");
    let set: HashSet<i32> = [7].into_iter().collect();
    assert_eq!(render(&set, 79), "HashSet::from([7])");
}

#[test]
fn smart_pointers_and_references_delegate() {
    assert_eq!(render(&Box::new(5), 79), "5");
    assert_eq!(render(&std::rc::Rc::new("x"), 79), "\"x\"");
    assert_eq!(render(&std::sync::Arc::new(vec![1]), 79), "vec![1]");
    let mut value = 9;
    let reference = &mut value;
    assert_eq!(render(&reference, 79), "9");
}

#[test]
fn durations_and_addresses_print_as_constructors() {
    assert_eq!(
        render(&Duration::from_millis(5500), 79),
        "Duration::new(5, 500000000)"
    );
    assert_eq!(
        render(&IpAddr::V4(Ipv4Addr::LOCALHOST), 79),
        "IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))"
    );
    assert_eq!(
        render(&Ipv4Addr::new(10, 0, 0, 1), 79),
        "Ipv4Addr::new(10, 0, 0, 1)"
    );
    assert_eq!(
        render(&Ipv6Addr::LOCALHOST, 79),
        "Ipv6Addr::new(0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1)"
    );
    assert_eq!(
        render(&IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)), 79),
        "IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1))"
    );
}

#[derive(Debug)]
struct DebugOnly {
    #[allow(dead_code)]
    x: i32,
}

#[derive(Debug)]
struct AlsoDebugOnly(#[allow(dead_code)] bool);

hegel::pretty_print_as_debug!(DebugOnly, AlsoDebugOnly);

struct MultiLineDebug;

impl std::fmt::Debug for MultiLineDebug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line one\nline two")
    }
}

hegel::pretty_print_as_debug!(MultiLineDebug);

#[test]
fn pretty_print_as_debug_reuses_debug_output() {
    assert_eq!(render(&DebugOnly { x: 1 }, 79), "DebugOnly { x: 1 }");
    assert_eq!(render(&AlsoDebugOnly(true), 79), "AlsoDebugOnly(true)");
}

#[test]
fn pretty_print_as_debug_honors_newlines_at_current_indentation() {
    assert_eq!(render(&MultiLineDebug, 79), "line one\nline two");

    let mut printer = PrettyPrinter::new(79);
    printer.shift_indent(2);
    MultiLineDebug.pretty_print(&mut printer);
    assert_eq!(printer.value(), "line one\n  line two");
}

#[test]
fn printer_text_treats_newlines_as_hard_breaks() {
    let mut printer = PrettyPrinter::new(79);
    printer.shift_indent(4);
    printer.text("a\nb");
    printer.shift_indent(-4);
    printer.hard_break();
    printer.text("c");
    assert_eq!(printer.value(), "a\n    b\nc");
}

#[test]
fn printer_groups_lay_out_inline_or_broken() {
    let mut printer = PrettyPrinter::new(79);
    printer.begin_group(1, "[");
    printer.text("1,");
    printer.breakable(" ");
    printer.text("2");
    printer.end_group(1, "]");
    assert_eq!(printer.value(), "[1, 2]");
}

#[test]
fn deferred_holes_fill_in_before_rendering() {
    let mut printer = PrettyPrinter::new(79);
    printer.text("a");
    let mut slot = printer.deferred();
    printer.text("d");
    slot.text("b\nc");
    assert_eq!(printer.value(), "ab\ncd");
}

#[test]
fn dead_deferred_slots_ignore_writes() {
    let mut printer = PrettyPrinter::new(79);
    printer.text("a");
    let mut slot = printer.deferred();
    slot.text("b");
    assert_eq!(printer.value(), "ab");
    slot.text("ignored");
    slot.breakable(" ");
    assert_eq!(printer.value(), "ab");
}

#[test]
fn comments_attach_to_line_ends_and_break_open_groups() {
    let mut printer = PrettyPrinter::new(79);
    printer.begin_group(1, "[");
    printer.text("1,");
    printer.breakable(" ");
    printer.text("2");
    printer.comment("or any other generated value");
    printer.text(",");
    printer.breakable(" ");
    printer.text("3");
    printer.end_group(1, "]");
    assert_eq!(
        printer.value(),
        "[1,\n 2,  // or any other generated value\n 3\n]"
    );
}

#[test]
fn comments_outside_groups_do_not_affect_layout() {
    let mut printer = PrettyPrinter::new(79);
    printer.text("let x = 0;");
    printer.comment("or any other generated value");
    printer.hard_break();
    printer.text("let y = 1;");
    assert_eq!(
        printer.value(),
        "let x = 0;  // or any other generated value\nlet y = 1;"
    );
}

#[test]
#[should_panic(expected = "must not contain newlines")]
fn comments_with_newlines_panic() {
    let mut printer = PrettyPrinter::new(79);
    printer.comment("a\nb");
}

#[test]
fn printer_debug_form_is_opaque() {
    let printer = PrettyPrinter::new(79);
    assert_eq!(
        format!("{printer:?}"),
        "PrettyPrinter { handle: Some(PrinterHandle { .. }) }"
    );
}

#[test]
#[should_panic(expected = "matching begin_group")]
fn unbalanced_end_group_panics() {
    let mut printer = PrettyPrinter::new(79);
    printer.end_group(0, "]");
}

mod debug_repr {
    use super::render;
    use hegel::pretty::print_debug_repr;
    use hegel::{PrettyPrintable, PrettyPrinter};

    fn render_debug(repr: &str, max_width: usize) -> String {
        let mut printer = PrettyPrinter::new(max_width);
        print_debug_repr(repr, &mut printer);
        printer.value()
    }

    #[test]
    fn flat_shapes_render_unchanged() {
        for repr in [
            "42",
            "Name",
            "10.5s",
            "1:30:00",
            "Point { x: 1, y: 2 }",
            "Some(5)",
            "(1, false)",
            "[1, 2, 3]",
            "{\"a\": 1, \"b\": 2}",
            "Wrapper([1, 2], 'x')",
            "\"quoted, [text]\"",
            "'\\''",
            "{}",
            "[]",
            "Unitish {}",
            "Outer { inner: Inner { n: 1 } }",
        ] {
            assert_eq!(render_debug(repr, 79), repr, "{repr}");
        }
    }

    #[test]
    fn parsed_groups_break_when_narrow() {
        assert_eq!(render_debug("[100, 200]", 6), "[100,\n 200]");
        assert_eq!(
            render_debug("Point { x: 100, y: 200 }", 12),
            "Point {\n    x: 100,\n    y: 200 }"
        );
    }

    #[test]
    fn unparseable_debug_output_is_emitted_verbatim() {
        for repr in [
            "unbalanced [100, 200",
            "don't",
            "\"unterminated",
            "top, level",
            "{ braced }",
            "extra ] close",
            "[100, 200] trailing, text",
            "[100,200 }",
        ] {
            assert_eq!(render_debug(repr, 6), repr, "{repr}");
        }
        assert_eq!(render_debug("multi\nline [1, 2]", 6), "multi\nline [1, 2]");
    }

    #[derive(Debug, PrettyPrintable)]
    struct Nested {
        name: String,
        values: [i32; 5],
        pair: (bool, char),
    }

    #[test]
    fn debug_repr_layout_matches_the_derive() {
        let value = Nested {
            name: "abcdef".to_string(),
            values: [100, 200, 300, 400, 500],
            pair: (true, 'x'),
        };
        for width in [10, 20, 30, 45, 79] {
            assert_eq!(
                render_debug(&format!("{value:?}"), width),
                render(&value, width),
                "width {width}"
            );
        }
    }
}

#[test]
fn should_print_distinguishes_real_and_noop_printers() {
    assert!(PrettyPrinter::new(79).should_print());
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
    printer.end_group(1, "]");
    let mut slot = printer.deferred();
    slot.text("later\ntext");
    slot.breakable(" ");
    assert_eq!(printer.value(), "");
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
    assert_eq!(printer.value(), "");
}
