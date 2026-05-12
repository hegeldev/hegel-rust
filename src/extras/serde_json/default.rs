use std::collections::HashMap;

use serde_json::{Map, Number, Value};

use crate::generators::{self as gs, BoxedGenerator, DefaultGenerator, Generator};

use super::{NumberGenerator, ValueGenerator, numbers, values};

impl DefaultGenerator for Number {
    type Generator = NumberGenerator;
    fn default_generator() -> Self::Generator {
        numbers()
    }
}

impl DefaultGenerator for Value {
    type Generator = ValueGenerator;
    fn default_generator() -> Self::Generator {
        values()
    }
}

// `serde_json::Map<K, V>` is generic in its type signature, but every public
// constructor and trait impl is hardcoded to `Map<String, Value>`. There's no
// way to construct any other instantiation through the crate's public API,
// so this impl covers the full usable space.
impl DefaultGenerator for Map<String, Value> {
    type Generator = BoxedGenerator<'static, Map<String, Value>>;
    fn default_generator() -> Self::Generator {
        gs::hashmaps(
            <String as DefaultGenerator>::default_generator(),
            <Value as DefaultGenerator>::default_generator(),
        )
        .map(|m: HashMap<String, Value>| m.into_iter().collect::<Map<String, Value>>())
        .boxed()
    }
}

#[cfg(feature = "serde_json_raw_value")]
impl DefaultGenerator for Box<serde_json::value::RawValue> {
    type Generator = super::RawValueGenerator;
    fn default_generator() -> Self::Generator {
        super::raw_values()
    }
}
