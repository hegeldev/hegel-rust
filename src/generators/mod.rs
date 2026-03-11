mod collections;
mod combinators;
mod compose;
mod data;
mod from_type;
#[allow(clippy::module_inception)]
mod generators;
mod misc;
mod numeric;
#[cfg(feature = "rand")]
mod random;
mod strings;
mod tuples;
mod value;

// public api: Generator<T> and DefaultGenerator are the primary user-facing types.
// Factory functions (integers(), text(), etc.) return concrete types for builder
// method chaining, but those concrete types are #[doc(hidden)] — users should
// use Generator<T> whenever they need a type annotation.
pub use generators::Generator;

// factory functions
pub use collections::{arrays, fixed_dicts, hashmaps, hashsets, vecs};
pub use combinators::{one_of, optional, sampled_from};
pub use misc::{booleans, just, none, unit};
pub use numeric::{floats, integers};
pub use strings::{
    binary, dates, datetimes, domains, emails, from_regex, ip_addresses, text, times, urls,
};
pub use tuples::{
    tuples10, tuples11, tuples12, tuples2, tuples3, tuples4, tuples5, tuples6, tuples7, tuples8,
    tuples9,
};
#[cfg(feature = "rand")]
#[cfg_attr(docsrs, doc(cfg(feature = "rand")))]
pub use random::randoms;

// types users may need to name
pub use from_type::{from_type, DefaultGenerator};
#[cfg(feature = "rand")]
#[cfg_attr(docsrs, doc(cfg(feature = "rand")))]
pub use random::HegelRandom;

// concrete generator types: public for type system reasons, but hidden from docs.
// Users should use Generator<T> instead of naming these directly.
#[doc(hidden)]
pub use collections::HashMapGenerator;
#[doc(hidden)]
pub use compose::{fnv1a_hash, ComposedGenerator};
#[doc(hidden)]
pub use data::{deserialize_value, labels, Collection, StopTestError, TestCaseData};
#[doc(hidden)]
pub use generators::{BasicGenerator, BoxedGenerator, Filtered, FlatMapped, Mapped};
#[cfg(feature = "rand")]
#[doc(hidden)]
pub use random::RandomsGenerator;

pub(crate) use collections::VecGenerator;
pub(crate) use combinators::OptionalGenerator;
pub(crate) use misc::BoolGenerator;
pub(crate) use numeric::{FloatGenerator, IntegerGenerator};
pub(crate) use strings::TextGenerator;

// Re-export for macros
#[doc(hidden)]
pub use crate::control::test_case_data;
