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

// public api
pub use collections::{arrays, fixed_dicts, hashmaps, hashsets, vecs, HashMapGenerator};
pub use combinators::{one_of, optional, sampled_from};
pub use compose::{fnv1a_hash, ComposedGenerator};
pub use data::{deserialize_value, labels, Collection, StopTestError, TestCaseData};
pub use from_type::{from_type, DefaultGenerator};
pub use generators::{BasicGenerator, BoxedGenerator, Filtered, FlatMapped, Generator, Mapped};
pub use misc::{booleans, just, none, unit};
pub use numeric::{floats, integers};
#[cfg(feature = "rand")]
#[cfg_attr(docsrs, doc(cfg(feature = "rand")))]
pub use random::{randoms, HegelRandom, RandomsGenerator};
pub use strings::{
    binary, dates, datetimes, domains, emails, from_regex, ip_addresses, text, times, urls,
};
pub use tuples::{
    tuples10, tuples11, tuples12, tuples2, tuples3, tuples4, tuples5, tuples6, tuples7, tuples8,
    tuples9,
};

pub(crate) use collections::VecGenerator;
pub(crate) use combinators::OptionalGenerator;
pub(crate) use misc::BoolGenerator;
pub(crate) use numeric::{FloatGenerator, IntegerGenerator};
pub(crate) use strings::TextGenerator;

// Re-export for macros
#[doc(hidden)]
pub use crate::control::test_case_data;
