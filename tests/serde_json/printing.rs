use crate::common::utils::printed_draw_lines;
use hegel::extras::serde_json as json_gs;

#[test]
fn every_serde_json_generator_prints_its_drawn_value() {
    printed_draw_lines(json_gs::numbers());
    printed_draw_lines(json_gs::values());
}

#[cfg(feature = "serde_json_raw_value")]
#[test]
fn raw_values_print_their_drawn_value() {
    printed_draw_lines(json_gs::raw_values());
}

fn render<T: hegel::PrettyPrintable + ?Sized>(value: &T) -> String {
    let mut printer = hegel::PrettyPrinter::new(79);
    value.pretty_print(&mut printer);
    printer.value()
}

#[test]
fn json_values_print_in_json_macro_syntax() {
    use serde_json::{Number, json};

    let value = json!({"a": [1, null, "x"], "b": true});
    assert_eq!(
        render(&value),
        "json!({\"a\": [1, null, \"x\"], \"b\": true})"
    );
    assert_eq!(render(&Number::from(-3)), "Number::from(-3)");
    assert_eq!(
        render(&Number::from(u64::MAX)),
        format!("Number::from({}u64)", u64::MAX)
    );
    assert_eq!(
        render(&Number::from_f64(1.5).unwrap()),
        "Number::from_f64(1.5).unwrap()"
    );
}

#[cfg(feature = "serde_json_raw_value")]
#[test]
fn raw_values_print_as_from_string_expressions() {
    let raw = serde_json::value::RawValue::from_string("null".to_string()).unwrap();
    assert_eq!(
        render(&*raw),
        "RawValue::from_string(\"null\".to_string()).unwrap()"
    );
}
