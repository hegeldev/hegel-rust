use std::collections::HashMap;

use serde_json::{Number, Value};

use crate::generators::{self as gs, BoxedGenerator, DefaultGenerator, Generator};

/// Generator for [`serde_json::Number`] values. Created by [`numbers()`].
///
/// Produces `Number` values backed by an `i64`, `u64`, or finite `f64`.
/// JSON numbers cannot represent NaN or infinity, so the float branch is
/// constrained to finite values.
pub struct NumberGenerator {
    inner: BoxedGenerator<'static, Number>,
}

impl Generator<Number> for NumberGenerator {
    fn do_draw(&self, tc: &crate::TestCase) -> Number {
        self.inner.do_draw(tc)
    }

    fn as_basic(&self) -> Option<gs::BasicGenerator<'_, Number>> {
        self.inner.as_basic()
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
        gs::integers::<i64>().map(Number::from).boxed(),
        gs::integers::<u64>().map(Number::from).boxed(),
        gs::floats::<f64>()
            .allow_nan(false)
            .allow_infinity(false)
            .map(|f| Number::from_f64(f).unwrap())
            .boxed(),
    ])
    .boxed();
    NumberGenerator { inner }
}

/// Generator for [`serde_json::Value`] values. Created by [`values()`].
pub struct ValueGenerator {
    inner: BoxedGenerator<'static, Value>,
}

impl Generator<Value> for ValueGenerator {
    fn do_draw(&self, tc: &crate::TestCase) -> Value {
        self.inner.do_draw(tc)
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

    let recursive = gs::one_of([
        gs::just(Value::Null).boxed(),
        gs::booleans().map(Value::Bool).boxed(),
        numbers().map(Value::Number).boxed(),
        <String as DefaultGenerator>::default_generator()
            .map(Value::String)
            .boxed(),
        gs::vecs(handle.clone()).map(Value::Array).boxed(),
        gs::hashmaps(
            <String as DefaultGenerator>::default_generator(),
            handle.clone(),
        )
        .map(|m: HashMap<String, Value>| Value::Object(m.into_iter().collect()))
        .boxed(),
    ])
    .boxed();

    def.set(recursive);

    ValueGenerator { inner: handle }
}

/// Generator for [`Box<RawValue>`](serde_json::value::RawValue) values.
/// Created by [`raw_values()`].
///
/// The generated values are guaranteed to be valid json.
#[cfg(feature = "serde_json_raw_value")]
pub struct RawValueGenerator {
    inner: BoxedGenerator<'static, Box<serde_json::value::RawValue>>,
}

#[cfg(feature = "serde_json_raw_value")]
impl Generator<Box<serde_json::value::RawValue>> for RawValueGenerator {
    fn do_draw(&self, tc: &crate::TestCase) -> Box<serde_json::value::RawValue> {
        self.inner.do_draw(tc)
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
        .boxed();
    RawValueGenerator { inner }
}
