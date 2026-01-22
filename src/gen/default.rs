use super::{
    booleans, floats, integers, optional, text, vecs, BoolGenerator, FloatGenerator,
    IntegerGenerator, OptionalGenerator, TextGenerator, VecGenerator,
};

/// Trait for types that have a default generator.
///
/// This is used by derive macros to automatically generate values for fields.
#[allow(dead_code)] // Used by derive macros in generated code
pub trait DefaultGenerator: Sized {
    /// The generator type for this type.
    type Generator: super::Generate<Self>;

    /// Get the default generator for this type.
    fn default_generator() -> Self::Generator;
}

impl DefaultGenerator for bool {
    type Generator = BoolGenerator;
    fn default_generator() -> Self::Generator {
        booleans()
    }
}

impl DefaultGenerator for String {
    type Generator = TextGenerator;
    fn default_generator() -> Self::Generator {
        text()
    }
}

impl DefaultGenerator for i8 {
    type Generator = IntegerGenerator<i8>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for i16 {
    type Generator = IntegerGenerator<i16>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for i32 {
    type Generator = IntegerGenerator<i32>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for i64 {
    type Generator = IntegerGenerator<i64>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for u8 {
    type Generator = IntegerGenerator<u8>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for u16 {
    type Generator = IntegerGenerator<u16>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for u32 {
    type Generator = IntegerGenerator<u32>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for u64 {
    type Generator = IntegerGenerator<u64>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for f32 {
    type Generator = FloatGenerator<f32>;
    fn default_generator() -> Self::Generator {
        floats()
    }
}

impl DefaultGenerator for f64 {
    type Generator = FloatGenerator<f64>;
    fn default_generator() -> Self::Generator {
        floats()
    }
}

impl<T: DefaultGenerator> DefaultGenerator for Option<T>
where
    T: serde::de::DeserializeOwned,
{
    type Generator = OptionalGenerator<T::Generator>;
    fn default_generator() -> Self::Generator {
        optional(T::default_generator())
    }
}

impl<T: DefaultGenerator> DefaultGenerator for Vec<T>
where
    T: serde::de::DeserializeOwned,
{
    type Generator = VecGenerator<T::Generator>;
    fn default_generator() -> Self::Generator {
        vecs(T::default_generator())
    }
}
