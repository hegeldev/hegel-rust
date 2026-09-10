use super::*;

fn render_counted(repr: &str, name: &str) -> String {
    let mut doc = crate::Document::new();
    print_counted_constructor(repr, name, doc.printer());
    doc.finish()
}

#[test]
fn counted_constructor_reprs_print_as_new_expressions() {
    assert_eq!(render_counted("Days(5)", "Days"), "Days::new(5)");
    assert_eq!(render_counted("Months(0)", "Months"), "Months::new(0)");
}

#[test]
fn unrecognized_counted_reprs_print_verbatim() {
    assert_eq!(render_counted("Days { n: 5 }", "Days"), "Days { n: 5 }");
}
