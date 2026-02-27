//! Declarative macro for deriving generators for external types.

/// Derive a generator for a struct type defined externally.
///
/// This macro creates a hidden generator struct with builder methods for each field,
/// and implements [`DefaultGenerator`](crate::generators::DefaultGenerator) for the type
/// so it can be used with [`from_type`](crate::generators::from_type).
///
/// # Example
///
/// ```ignore
/// // In your production crate (no hegel dependency needed):
/// pub struct Person {
///     pub name: String,
///     pub age: u32,
/// }
///
/// // In your test crate:
/// use hegel::derive_generator;
/// use hegel::generators::{self, Generate};
/// use production_crate::Person;
///
/// derive_generator!(Person {
///     name: String,
///     age: u32,
/// });
///
/// // Use from_type to get a generator:
/// let gen = generators::from_type::<Person>()
///     .with_name(generators::from_regex("[A-Z][a-z]+"))
///     .with_age(generators::integers::<u32>().with_min(0).with_max(120));
///
/// let person: Person = hegel::draw(&gen);
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
                        use $crate::generators::{DefaultGenerator, Generate};
                        Self {
                            $($field_name: <$field_type as DefaultGenerator>::default_generator().boxed(),)*
                        }
                    }

                    $(
                        pub fn [<with_ $field_name>]<G>(mut self, gen: G) -> Self
                        where
                            G: $crate::generators::Generate<$field_type> + Send + Sync + 'a,
                        {
                            use $crate::generators::Generate;
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

                impl<'a> $crate::generators::Generate<$struct_name> for [<$struct_name Generator>]<'a> {
                    fn do_draw(&self, __data: &$crate::generators::TestCaseData) -> $struct_name {
                        use $crate::generators::Generate;
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
