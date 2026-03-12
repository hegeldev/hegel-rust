use super::{
    booleans, collections::ArrayGenerator, floats, hashmaps, integers, optional, text, vecs,
    BoolGenerator, FloatGenerator, HashMapGenerator, IntegerGenerator, OptionalGenerator,
    TextGenerator, VecGenerator,
};
use std::collections::HashMap;
use std::hash::Hash;

/// Trait for types that have a default generator.
///
/// This is used by derive macros to automatically generate values for fields.
pub trait DefaultGenerator: Sized {
    type Generator: super::Generator<Self>;
    fn default_generator() -> Self::Generator;
}

/// Create a generator for a type using its default generator.
///
/// This is the primary way to get a generator for types that implement
/// [`DefaultGenerator`], including types with `#[derive(Generator)]`.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self, DefaultGenerator};
/// use hegel::Generator;
///
/// #[derive(Generator, Debug)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// #[hegel::test]
/// fn my_test(tc: &hegel::TestCase) {
///     // Generate with defaults
///     let person: Person = tc.draw(&generators::from_type::<Person>());
///
///     // Customize field generators
///     let person: Person = tc.draw(&generators::from_type::<Person>()
///         .with_age(generators::integers().min_value(0).max_value(120)));
/// }
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

impl DefaultGenerator for i128 {
    type Generator = IntegerGenerator<i128>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for u128 {
    type Generator = IntegerGenerator<u128>;
    fn default_generator() -> Self::Generator {
        integers()
    }
}

impl DefaultGenerator for isize {
    type Generator = IntegerGenerator<isize>;
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

impl<T: DefaultGenerator, const N: usize> DefaultGenerator for [T; N]
where
    T::Generator: Send + Sync,
{
    type Generator = ArrayGenerator<T::Generator, T, N>;
    fn default_generator() -> Self::Generator {
        ArrayGenerator::new(T::default_generator())
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

/// Derive a generator for a struct type defined externally.
///
/// This macro creates a hidden generator struct with builder methods for each field,
/// and implements [`DefaultGenerator`](crate::generators::DefaultGenerator) for the type
/// so it can be used with [`from_type`](crate::generators::from_type).
///
/// # Example
///
/// ```ignore
/// // In your crate:
/// pub struct Person {
///     pub name: String,
///     pub age: u32,
/// }
///
/// // In your tests:
/// use hegel::derive_generator;
/// use hegel::generators::{self, DefaultGenerator, Generator};
/// use production_crate::Person;
///
/// derive_generator!(Person {
///     name: String,
///     age: u32,
/// });
///
/// // from_type now supports Person:
/// let gen = generators::from_type::<Person>()
///     .with_name(generators::from_regex("[A-Z][a-z]+"))
///     .with_age(generators::integers::<u32>().min_value(0).max_value(120));
///
/// let person: Person = tc.draw(&gen);
/// ```
#[macro_export]
macro_rules! derive_generator {
    ($struct_name:ident { $($field_name:ident : $field_type:ty),* $(,)? }) => {
        const _: () = {
            $crate::paste::paste! {
                pub struct [<$struct_name Generator>]<'a> {
                    $(
                        $field_name: $crate::generators::BoxedGenerator<'a, $field_type>,
                    )*
                }

                impl<'a> [<$struct_name Generator>]<'a> {
                    pub fn new() -> Self
                    where
                        $($field_type: $crate::generators::DefaultGenerator,)*
                        $(<$field_type as $crate::generators::DefaultGenerator>::Generator: Send + Sync + 'a,)*
                    {
                        use $crate::generators::{DefaultGenerator, Generator};
                        Self {
                            $($field_name: <$field_type as DefaultGenerator>::default_generator().boxed(),)*
                        }
                    }

                    $(
                        pub fn [<with_ $field_name>]<G>(mut self, gen: G) -> Self
                        where
                            G: $crate::generators::Generator<$field_type> + Send + Sync + 'a,
                        {
                            use $crate::generators::Generator;
                            self.$field_name = gen.boxed();
                            self
                        }
                    )*
                }

                impl<'a> Default for [<$struct_name Generator>]<'a>
                where
                    $($field_type: $crate::generators::DefaultGenerator,)*
                    $(<$field_type as $crate::generators::DefaultGenerator>::Generator: Send + Sync + 'a,)*
                {
                    fn default() -> Self {
                        Self::new()
                    }
                }

                impl<'a> $crate::generators::Generator<$struct_name> for [<$struct_name Generator>]<'a> {
                    fn do_draw(&self, __data: &$crate::TestCase) -> $struct_name {
                        use $crate::generators::Generator;
                        $struct_name {
                            $($field_name: self.$field_name.do_draw(__data),)*
                        }
                    }
                }

                impl $crate::generators::DefaultGenerator for $struct_name
                where
                    $($field_type: $crate::generators::DefaultGenerator,)*
                    $(<$field_type as $crate::generators::DefaultGenerator>::Generator: Send + Sync + 'static,)*
                {
                    type Generator = [<$struct_name Generator>]<'static>;
                    fn default_generator() -> Self::Generator {
                        [<$struct_name Generator>]::new()
                    }
                }
            }
        };
    };
}
