use crate::generators::binary;

use super::{
    BoolGenerator, BoxedGenerator, CharactersGenerator, DurationGenerator, FloatGenerator,
    Generator, HashMapGenerator, HashSetGenerator, IntegerGenerator, IpAddressGenerator,
    Ipv4AddressGenerator, Ipv6AddressGenerator, OptionalGenerator, TextGenerator, VecGenerator,
    booleans, characters, collections::ArrayGenerator, durations, floats, hashmaps, hashsets,
    integers, ip_addresses, optional, text, vecs,
};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::time::Duration;

/// Trait for types that have a default generator.
///
/// This is used by derive macros to automatically generate values for fields.
pub trait DefaultGenerator: Sized {
    type Generator: super::Generator<Self> + 'static;
    fn default_generator() -> Self::Generator;
}

/// Create a generator for a type using its default generator.
///
/// This is the primary way to get a generator for types that implement
/// [`DefaultGenerator`], including types with `#[derive(DefaultGenerator)]`.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self as gs, DefaultGenerator};
/// use hegel::DefaultGenerator;
///
/// #[derive(DefaultGenerator)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     // Generate with defaults
///     let person: Person = tc.draw(gs::default::<Person>());
///
///     // Customize field generators
///     let person: Person = tc.draw(gs::default::<Person>()
///         .age(gs::integers().min_value(0).max_value(120)));
/// }
/// ```
pub fn default<T: DefaultGenerator>() -> T::Generator {
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

impl DefaultGenerator for char {
    type Generator = CharactersGenerator;
    fn default_generator() -> Self::Generator {
        characters()
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

impl<T: DefaultGenerator + 'static> DefaultGenerator for Option<T>
where
    T::Generator: Send + Sync,
{
    type Generator = OptionalGenerator<T::Generator, T>;
    fn default_generator() -> Self::Generator {
        optional(T::default_generator())
    }
}

impl<T: DefaultGenerator + 'static> DefaultGenerator for Vec<T>
where
    T::Generator: Send + Sync,
{
    type Generator = VecGenerator<T::Generator, T>;
    fn default_generator() -> Self::Generator {
        vecs(T::default_generator())
    }
}

impl<T: DefaultGenerator + 'static, const N: usize> DefaultGenerator for [T; N]
where
    T::Generator: Send + Sync,
{
    type Generator = ArrayGenerator<T::Generator, T, N>;
    fn default_generator() -> Self::Generator {
        ArrayGenerator::new(T::default_generator())
    }
}

impl DefaultGenerator for Duration {
    type Generator = DurationGenerator;
    fn default_generator() -> Self::Generator {
        durations()
    }
}

impl DefaultGenerator for PathBuf {
    type Generator = BoxedGenerator<'static, PathBuf>;
    fn default_generator() -> Self::Generator {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            binary()
                .map(std::ffi::OsString::from_vec)
                .map(PathBuf::from)
                .boxed()
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            vecs(integers())
                .map(|wide: Vec<u16>| std::ffi::OsString::from_wide(&wide))
                .map(PathBuf::from)
                .boxed()
        }
        #[cfg(not(any(unix, windows)))]
        {
            text().map(PathBuf::from).boxed()
        }
    }
}

impl DefaultGenerator for IpAddr {
    type Generator = IpAddressGenerator;

    fn default_generator() -> Self::Generator {
        ip_addresses()
    }
}

impl DefaultGenerator for Ipv4Addr {
    type Generator = Ipv4AddressGenerator;

    fn default_generator() -> Self::Generator {
        ip_addresses().v4()
    }
}

impl DefaultGenerator for Ipv6Addr {
    type Generator = Ipv6AddressGenerator;

    fn default_generator() -> Self::Generator {
        ip_addresses().v6()
    }
}

impl<K: DefaultGenerator + 'static, V: DefaultGenerator + 'static> DefaultGenerator
    for HashMap<K, V>
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

impl<T: DefaultGenerator + 'static> DefaultGenerator for HashSet<T>
where
    T: Eq + Hash,
    T::Generator: Send + Sync,
{
    type Generator = HashSetGenerator<T::Generator, T>;
    fn default_generator() -> Self::Generator {
        hashsets(T::default_generator())
    }
}

/// Derive a generator for a struct defined in another crate.
///
/// [`#[derive(DefaultGenerator)]`](crate::DefaultGenerator) only works on
/// struct definitions you own. For an external struct whose fields are
/// public, `derive_generator!` instead generates a standalone generator
/// struct, named by the first argument, with:
/// - `new()` - construct the generator, using [`default`] for every field
/// - `<field>(generator)` - builder method to customize each field's generator
///
/// Rust's orphan rule prevents implementing [`DefaultGenerator`] for a type
/// from another crate, so [`default`] cannot support external types; draw
/// from the generated generator directly instead.
///
/// The generated generator is a
/// [`PrintableGenerator`](crate::PrintableGenerator), printing values as
/// `Name { field: value, … }` expressions, exactly when every field type
/// implements [`PrettyPrintable`](crate::PrettyPrintable); otherwise it can
/// only be drawn silently, or made printable with
/// [`print_as_debug`](crate::Generator::print_as_debug) or
/// [`print_with`](crate::Generator::print_with).
///
/// # Example
///
/// ```no_run
/// // Defined in another crate, so #[derive(DefaultGenerator)] can't be
/// // added to it:
/// pub mod production_crate {
///     #[derive(Debug)]
///     pub struct Person {
///         pub name: String,
///         pub age: u32,
///     }
/// }
/// use production_crate::Person;
///
/// use hegel::derive_generator;
/// use hegel::generators as gs;
///
/// derive_generator!(PersonGenerator for Person {
///     name: String,
///     age: u32,
/// });
///
/// #[hegel::test]
/// fn generates_people(tc: hegel::TestCase) {
///     let person: Person = tc.draw(
///         PersonGenerator::new()
///             .name(gs::from_regex("[A-Z][a-z]+"))
///             .age(gs::integers::<u32>().min_value(0).max_value(120)),
///     );
/// }
/// ```
#[macro_export]
macro_rules! derive_generator {
    ($gen_name:ident for $struct_type:path { $($field_name:ident : $field_type:ty),+ $(,)? }) => {
        pub struct $gen_name<'a> {
            $(
                $field_name: $crate::generators::BoxedGenerator<'a, $field_type>,
            )*
        }

        impl<'a> $gen_name<'a> {
            pub fn new() -> Self
            where
                $($field_type: $crate::generators::DefaultGenerator,)*
                $(<$field_type as $crate::generators::DefaultGenerator>::Generator: Send + Sync + 'a,)*
            {
                Self {
                    $($field_name: $crate::generators::Generator::boxed(
                        <$field_type as $crate::generators::DefaultGenerator>::default_generator(),
                    ),)*
                }
            }

            $(
                pub fn $field_name<G>(mut self, generator: G) -> Self
                where
                    G: $crate::generators::Generator<$field_type> + Send + Sync + 'a,
                {
                    self.$field_name = $crate::generators::Generator::boxed(generator);
                    self
                }
            )*
        }

        impl<'a> Default for $gen_name<'a>
        where
            $($field_type: $crate::generators::DefaultGenerator,)*
            $(<$field_type as $crate::generators::DefaultGenerator>::Generator: Send + Sync + 'a,)*
        {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<'a> $crate::generators::Generator<$struct_type> for $gen_name<'a> {
            fn do_draw(&self, __tc: &$crate::TestCase) -> $struct_type {
                $struct_type {
                    $($field_name: $crate::generators::Generator::do_draw(&self.$field_name, __tc),)*
                }
            }
        }

        impl<'a> $crate::generators::PrintableGenerator<$struct_type> for $gen_name<'a>
        where
            $($field_type: $crate::PrettyPrintable,)*
        {
            fn do_draw_and_print(
                &self,
                __tc: &$crate::TestCase,
                __printer: &mut $crate::PrettyPrinter,
            ) -> $struct_type {
                __printer.begin_group(4, concat!(stringify!($struct_type), " {"));
                __printer.breakable(" ");
                let mut __first = true;
                $(
                    if !__first {
                        __printer.text(",");
                        __printer.breakable(" ");
                    }
                    __first = false;
                    __printer.text(concat!(stringify!($field_name), ": "));
                    let $field_name = __tc.draw_and_print(&self.$field_name, __printer);
                )*
                let _ = __first;
                __printer.end_group(" }");
                $struct_type { $($field_name,)* }
            }
        }
    };
}
