mod collections;
mod combinators;
mod compose;
mod default;
#[allow(clippy::module_inception)]
mod generators;
mod misc;
mod numeric;
#[cfg(feature = "rand")]
mod random;
mod strings;
mod tuples;
pub(crate) mod value;

// public api
pub use crate::test_case::{
    Collection, StopTestError, TestCase, deserialize_value, generate_from_schema, generate_raw,
    labels,
};
pub use collections::{HashMapGenerator, arrays, fixed_dicts, hashmaps, hashsets, vecs};
pub use combinators::{one_of, optional, sampled_from};
pub use compose::{ComposedGenerator, fnv1a_hash};
pub use default::{DefaultGenerator, default};
pub use generators::{BasicGenerator, BoxedGenerator, Filtered, FlatMapped, Generator, Mapped};
pub use misc::{booleans, just, none, unit};
pub use numeric::{floats, integers};
#[cfg(feature = "rand")]
#[cfg_attr(docsrs, doc(cfg(feature = "rand")))]
pub use random::{HegelRandom, RandomsGenerator, randoms};
pub use strings::{
    binary, dates, datetimes, domains, emails, from_regex, ip_addresses, text, times, urls,
};
pub use tuples::{
    tuples2, tuples3, tuples4, tuples5, tuples6, tuples7, tuples8, tuples9, tuples10, tuples11,
    tuples12,
};

pub(crate) use collections::VecGenerator;
pub(crate) use combinators::OptionalGenerator;
pub(crate) use misc::BoolGenerator;
pub(crate) use numeric::{FloatGenerator, IntegerGenerator};
pub(crate) use strings::TextGenerator;
