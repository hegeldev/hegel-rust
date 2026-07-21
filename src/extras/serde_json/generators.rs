use std::collections::HashMap;

use serde_json::{Number, Value};

use crate::generators::{
    self as gs, BoxedPrintableGenerator, DefaultGenerator, Generator, PrintableGenerator,
};
use crate::pretty::PrettyPrinter;

impl crate::PrettyPrintable for Number {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        if let Some(n) = self.as_i64() {
            printer.text(&format!("Number::from({n})"));
        } else if let Some(n) = self.as_u64() {
            printer.text(&format!("Number::from({n}u64)"));
        } else {
            printer.text(&format!(
                "Number::from_f64({:?}).unwrap()",
                self.as_f64().unwrap()
            ));
        }
    }
}

/// Print a [`Value`] in `json!` macro syntax: JSON literals for scalars, and
/// group-wrapped arrays and objects so large documents break one element per
/// line.
fn pretty_print_json(value: &Value, printer: &mut PrettyPrinter) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            printer.text(&value.to_string());
        }
        Value::Array(items) => {
            printer.begin_group(1, "[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    printer.text(",");
                    printer.breakable(" ");
                }
                pretty_print_json(item, printer);
            }
            printer.end_group(1, "]");
        }
        Value::Object(entries) => {
            printer.begin_group(1, "{");
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    printer.text(",");
                    printer.breakable(" ");
                }
                printer.text(&format!("{}: ", Value::String(key.clone())));
                pretty_print_json(item, printer);
            }
            printer.end_group(1, "}");
        }
    }
}

impl crate::PrettyPrintable for Value {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        printer.begin_group(6, "json!(");
        pretty_print_json(self, printer);
        printer.end_group(6, ")");
    }
}

#[cfg(feature = "serde_json_raw_value")]
impl crate::PrettyPrintable for serde_json::value::RawValue {
    fn pretty_print(&self, printer: &mut PrettyPrinter) {
        printer.text(&format!(
            "RawValue::from_string({:?}.to_string()).unwrap()",
            self.get()
        ));
    }
}

/// Generator for [`serde_json::Number`] values. Created by [`numbers()`].
///
/// Produces `Number` values backed by an `i64`, `u64`, or finite `f64`.
/// JSON numbers cannot represent NaN or infinity, so the float branch is
/// constrained to finite values.
pub struct NumberGenerator {
    inner: BoxedPrintableGenerator<'static, Number>,
}

impl Generator<Number> for NumberGenerator {
    fn do_draw(&self, tc: &crate::TestCase) -> Number {
        self.inner.do_draw(tc)
    }
}

impl PrintableGenerator<Number> for NumberGenerator {
    fn do_draw_and_print(&self, tc: &crate::TestCase, printer: &mut PrettyPrinter) -> Number {
        self.inner.do_draw_and_print(tc, printer)
    }
}

/// Generate [`serde_json::Number`] values.
///
/// See [`NumberGenerator`] for details.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::serde_json as json_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let n = tc.draw(json_gs::numbers());
///     assert!(n.as_i64().is_some() || n.as_u64().is_some() || n.as_f64().is_some());
/// }
/// ```
pub fn numbers() -> NumberGenerator {
    let inner = gs::one_of([
        gs::integers::<i64>().map(Number::from).boxed_printable(),
        gs::integers::<u64>().map(Number::from).boxed_printable(),
        gs::floats::<f64>()
            .allow_nan(false)
            .allow_infinity(false)
            .map(|f| Number::from_f64(f).unwrap())
            .boxed_printable(),
    ])
    .boxed_printable();
    NumberGenerator { inner }
}

/// Generator for [`serde_json::Value`] values. Created by [`values()`].
pub struct ValueGenerator {
    inner: BoxedPrintableGenerator<'static, Value>,
}

impl Generator<Value> for ValueGenerator {
    fn do_draw(&self, tc: &crate::TestCase) -> Value {
        self.inner.do_draw(tc)
    }
}

impl PrintableGenerator<Value> for ValueGenerator {
    fn do_draw_and_print(&self, tc: &crate::TestCase, printer: &mut PrettyPrinter) -> Value {
        self.inner.do_draw_and_print(tc, printer)
    }
}

/// Generate [`serde_json::Value`] values.
///
/// See [`ValueGenerator`] for details.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::serde_json as json_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let v = tc.draw(json_gs::values());
///     // round-trip through serde_json
///     let s = serde_json::to_string(&v).unwrap();
///     let _: serde_json::Value = serde_json::from_str(&s).unwrap();
/// }
/// ```
pub fn values() -> ValueGenerator {
    let def = gs::deferred::<Value>();
    let handle = def.generator();

    // The recursive branches must keep the branching process subcritical:
    // arrays and objects are each picked with probability 1/6, so with the
    // default unbounded collection sizes (mean length 5) every node would
    // expect ~1.67 children and a large fraction of trees would only
    // terminate by exhausting the choice buffer. Capping the per-level size
    // at 3 keeps the expected child count below 1, so trees die out
    // naturally (Hypothesis's `st.recursive` bounds its trees for the same
    // reason).
    let recursive = gs::one_of([
        gs::just(Value::Null).boxed_printable(),
        gs::booleans().map(Value::Bool).boxed_printable(),
        numbers().map(Value::Number).boxed_printable(),
        <String as DefaultGenerator>::default_generator()
            .map(Value::String)
            .boxed_printable(),
        gs::vecs(handle.clone())
            .max_size(3)
            .map(Value::Array)
            .boxed_printable(),
        gs::hashmaps(
            <String as DefaultGenerator>::default_generator(),
            handle.clone(),
        )
        .max_size(3)
        .map(|m: HashMap<String, Value>| Value::Object(m.into_iter().collect()))
        .boxed_printable(),
    ])
    .boxed_printable();

    def.set(recursive);

    ValueGenerator { inner: handle }
}

/// Generator for [`Box<RawValue>`](serde_json::value::RawValue) values.
/// Created by [`raw_values()`].
///
/// The generated values are guaranteed to be valid json.
#[cfg(feature = "serde_json_raw_value")]
pub struct RawValueGenerator {
    inner: BoxedPrintableGenerator<'static, Box<serde_json::value::RawValue>>,
}

#[cfg(feature = "serde_json_raw_value")]
impl Generator<Box<serde_json::value::RawValue>> for RawValueGenerator {
    fn do_draw(&self, tc: &crate::TestCase) -> Box<serde_json::value::RawValue> {
        self.inner.do_draw(tc)
    }
}

#[cfg(feature = "serde_json_raw_value")]
impl PrintableGenerator<Box<serde_json::value::RawValue>> for RawValueGenerator {
    fn do_draw_and_print(
        &self,
        tc: &crate::TestCase,
        printer: &mut PrettyPrinter,
    ) -> Box<serde_json::value::RawValue> {
        self.inner.do_draw_and_print(tc, printer)
    }
}

/// Generate [`Box<RawValue>`](serde_json::value::RawValue) values.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::serde_json as json_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let r = tc.draw(json_gs::raw_values());
///     // The generated value is always valid JSON.
///     let _: serde_json::Value = serde_json::from_str(r.get()).unwrap();
/// }
/// ```
#[cfg(feature = "serde_json_raw_value")]
pub fn raw_values() -> RawValueGenerator {
    let inner = values()
        .map(|v| {
            serde_json::value::RawValue::from_string(serde_json::to_string(&v).unwrap()).unwrap()
        })
        .boxed_printable();
    RawValueGenerator { inner }
}
