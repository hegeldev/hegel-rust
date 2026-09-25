use super::*;

use alloc::format;
use alloc::string::ToString;

fn render(repr: &str, max_width: usize) -> String {
    let mut printer = Printer::new(max_width);
    reflow(&mut printer, Target::Main, repr, &ReflowOptions::default()).unwrap();
    printer.value().unwrap().to_string()
}

#[test]
fn flat_shapes_render_unchanged_when_they_fit() {
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
        "odd  {1}",
        "main.Point{X:1, Y:2}",
        "[]int{1, 2, 3}",
        "map[string]int{\"a\":1, \"b\":2}",
        "&main.Point{X:1}",
        "Point(x=1, y=2)",
        "{'a': 1, 'b': [1, 2]}",
        "{ a: 1, b: [ 1, 2 ] }",
        "Map(2) { 'a' => 1, 'b' => 2 }",
        "Point[x=1, y=2]",
        "{a=1, b=2}",
        "{x = 1; y = 2}",
        "[1; 2; 3]",
        "Some (1, 2)",
    ] {
        assert_eq!(render(repr, 79), repr, "{repr}");
    }
}

#[test]
fn rust_shapes_break_one_item_per_line() {
    assert_eq!(render("[100, 200]", 6), "[100,\n 200]");
    assert_eq!(render("Some([100, 200])", 12), "Some([100,\n      200])");
    assert_eq!(
        render("Point { x: 100, y: 200 }", 12),
        "Point {\n    x: 100,\n    y: 200 }"
    );
    assert_eq!(
        render("Outer { m: {1: 2, 3: 4} }", 18),
        "Outer {\n    m: {1: 2,\n        3: 4} }"
    );
    assert_eq!(
        render("{\"key\": [1, 2], \"other\": 3}", 20),
        "{\"key\": [1, 2],\n \"other\": 3}"
    );
}

#[test]
fn go_shapes_align_past_the_type_expression() {
    assert_eq!(
        render("main.Point{X:100, Y:200}", 16),
        "main.Point{X:100,\n           Y:200}"
    );
    assert_eq!(render("[]int{100, 200}", 10), "[]int{100,\n      200}");
    assert_eq!(
        render("map[string]int{\"a\":1, \"b\":2}", 20),
        "map[string]int{\"a\":1,\n               \"b\":2}"
    );
    assert_eq!(
        render("[]main.P{main.P{X:1}, main.P{X:2}}", 20),
        "[]main.P{main.P{X:1},\n         main.P{X:2}}"
    );
}

#[test]
fn python_shapes_break_like_rust_ones() {
    assert_eq!(
        render("Point(x=100, y=200)", 12),
        "Point(x=100,\n      y=200)"
    );
    assert_eq!(render("{'a': 100, 'b': 200}", 12), "{'a': 100,\n 'b': 200}");
    assert_eq!(render("(100, 200)", 6), "(100,\n 200)");
}

#[test]
fn padded_javascript_shapes_break_in_block_style() {
    assert_eq!(
        render("{ a: 100, b: 200 }", 12),
        "{\n    a: 100,\n    b: 200 }"
    );
    assert_eq!(render("[ 100, 200 ]", 8), "[\n    100,\n    200 ]");
    assert_eq!(
        render("Map(2) { 'a' => 1, 'b' => 2 }", 16),
        "Map(2) {\n    'a' => 1,\n    'b' => 2 }"
    );
}

#[test]
fn java_and_ocaml_shapes_break_at_their_separators() {
    assert_eq!(
        render("Point[x=100, y=200]", 12),
        "Point[x=100,\n      y=200]"
    );
    assert_eq!(render("{a=100, b=200}", 10), "{a=100,\n b=200}");
    assert_eq!(render("{x = 100; y = 200}", 12), "{x = 100;\n y = 200}");
    assert_eq!(render("[100; 200; 300]", 8), "[100;\n 200;\n 300]");
}

#[test]
fn separators_inside_quoted_literals_do_not_split_items() {
    assert_eq!(render("[\"a, b\", \"c; d\"]", 10), "[\"a, b\",\n \"c; d\"]");
    assert_eq!(
        render("['x, y', \"q\\\"; r\"]", 10),
        "['x, y',\n \"q\\\"; r\"]"
    );
}

#[test]
fn absurdly_nested_representations_fall_back_to_verbatim() {
    let repr = format!("{}0{}", "(".repeat(5000), ")".repeat(5000));
    assert_eq!(render(&repr, 79), repr);
}

#[test]
fn unparseable_representations_are_emitted_verbatim() {
    for repr in [
        "unbalanced [100, 200",
        "don't",
        "\"unterminated",
        "top, level",
        "extra ] close",
        "[100, 200] trailing, text",
        "[100,200 }",
        "[100 ]",
        "{ }",
    ] {
        assert_eq!(render(repr, 6), repr, "{repr}");
    }
}

#[test]
fn multi_line_representations_are_emitted_verbatim_with_hard_breaks() {
    assert_eq!(render("multi\nline [1, 2]", 6), "multi\nline [1, 2]");
    let mut printer = Printer::new(79);
    printer.shift_indent(Target::Main, 2).unwrap();
    reflow(
        &mut printer,
        Target::Main,
        "line one\n\nline three",
        &ReflowOptions::default(),
    )
    .unwrap();
    assert_eq!(printer.value().unwrap(), "line one\n  \n  line three");
}

#[test]
fn a_deep_group_is_not_folded_into_a_prefix() {
    assert_eq!(render("[[1]]x{100, 200}", 10), "[[1]]x{100,\n  200}");
}

#[test]
fn reflow_writes_to_a_deferred_slot() {
    let mut printer = Printer::new(79);
    printer.text(Target::Main, "let x = ").unwrap();
    let slot = printer.deferred(Target::Main).unwrap();
    printer.text(Target::Main, ";").unwrap();
    reflow(
        &mut printer,
        Target::Slot(slot),
        "Point { x: 1 }",
        &ReflowOptions::default(),
    )
    .unwrap();
    printer.resolve().unwrap();
    assert_eq!(printer.value().unwrap(), "let x = Point { x: 1 };");
}

#[test]
fn reflow_into_a_dead_slot_reports_the_dead_slot() {
    let mut printer = Printer::new(79);
    let slot = printer.deferred(Target::Main).unwrap();
    printer.resolve().unwrap();
    assert_eq!(
        reflow(
            &mut printer,
            Target::Slot(slot),
            "[1, 2]",
            &ReflowOptions::default()
        ),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(
        reflow(
            &mut printer,
            Target::Slot(slot),
            "a\nb",
            &ReflowOptions::default()
        ),
        Err(PrinterError::DeadSlot)
    );
}
