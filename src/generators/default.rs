use super::{
    booleans, floats, hashmaps, integers, optional, text, vecs, BoolGenerator, FloatGenerator,
    HashMapGenerator, IntegerGenerator, OptionalGenerator, TextGenerator, VecGenerator,
};
use std::collections::HashMap;
use std::hash::Hash;

/// Trait for types that have a default generator.
///
/// This is used by derive macros to automatically generate values for fields.
pub trait DefaultGenerator: Sized {
    /// The generator type for this type.
    type Generator: super::Generate<Self>;

    /// Get the default generator for this type.
    fn default_generator() -> Self::Generator;
}

/// Create a generator for a type using its default generator.
///
/// This is the primary way to get a generator for types that implement
/// [`DefaultGenerator`], including types with `#[derive(Generate)]`.
///
/// # Example
///
/// ```no_run
/// use hegel::generators;
/// use hegel::Generate;
///
/// #[derive(Generate, Debug)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// # hegel::hegel(|| {
/// // Generate with defaults
/// let person: Person = hegel::draw(&generators::from_type::<Person>());
///
/// // Customize field generators
/// let person: Person = hegel::draw(&generators::from_type::<Person>()
///     .with_age(generators::integers().with_min(0).with_max(120)));
/// # });
/// ```
pub fn from_type<T: DefaultGenerator>() -> T::Generator {
    T::default_generator()
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

impl DefaultGenerator for usize {
    type Generator = IntegerGenerator<usize>;
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
    T::Generator: Send + Sync,
{
    type Generator = OptionalGenerator<T::Generator, T>;
    fn default_generator() -> Self::Generator {
        optional(T::default_generator())
    }
}

impl<T: DefaultGenerator> DefaultGenerator for Vec<T>
where
    T::Generator: Send + Sync,
{
    type Generator = VecGenerator<T::Generator, T>;
    fn default_generator() -> Self::Generator {
        vecs(T::default_generator())
    }
}

impl<K: DefaultGenerator, V: DefaultGenerator> DefaultGenerator for HashMap<K, V>
where
    K: Eq + Hash,
    K::Generator: Send + Sync,
    V::Generator: Send + Sync,
{
    type Generator = HashMapGenerator<K::Generator, V::Generator, K, V>;
    fn default_generator() -> Self::Generator {
        hashmaps(K::default_generator(), V::default_generator())
    }
}
