//! Generators for producing test data.
//!
//! Each generator is created via a factory function (e.g. [`integers()`], [`text()`])
//! that returns a builder struct. Most builders have methods for constraining the
//! output (e.g. `min_value`, `max_size`). All builders implement [`Generator<T>`],
//! which provides combinators like [`map`](Generator::map), [`filter`](Generator::filter),
//! and [`flat_map`](Generator::flat_map).

mod collections;
mod combinators;
mod compose;
mod default;
#[allow(clippy::module_inception)]
mod generators;
mod misc;
mod numeric;
mod strings;
mod tuples;

#[cfg(feature = "rand")]
mod random;

pub(crate) mod value;

#[doc(hidden)]
pub use crate::test_case::{
    Collection, StopTestError, TestCase, deserialize_value, generate_from_schema, generate_raw,
    labels,
};

// public api
#[doc(inline)]
pub use crate::tuples;
pub use collections::{
    ArrayGenerator, FixedDictBuilder, FixedDictGenerator, HashMapGenerator, HashSetGenerator,
    VecGenerator, arrays, fixed_dicts, hashmaps, hashsets, vecs,
};
pub use combinators::{
    OneOfGenerator, OptionalGenerator, SampledFromGenerator, one_of, optional, sampled_from,
};
pub use compose::ComposedGenerator;
#[doc(hidden)]
pub use compose::fnv1a_hash;
pub use default::{DefaultGenerator, default};
#[doc(hidden)]
pub use generators::BasicGenerator;
pub use generators::{BoxedGenerator, Filtered, FlatMapped, Generator, Mapped};
pub use misc::{BoolGenerator, JustGenerator, booleans, just, unit};
pub use numeric::{Float, FloatGenerator, Integer, IntegerGenerator, floats, integers};
pub use strings::{
    BinaryGenerator, CharactersGenerator, DateGenerator, DateTimeGenerator, DomainGenerator,
    EmailGenerator, IpAddressGenerator, RegexGenerator, TextGenerator, TimeGenerator, UrlGenerator,
    binary, characters, dates, datetimes, domains, emails, from_regex, ip_addresses, text, times,
    urls,
};
#[doc(hidden)]
pub use tuples::{
    tuples0, tuples1, tuples2, tuples3, tuples4, tuples5, tuples6, tuples7, tuples8, tuples9,
    tuples10, tuples11, tuples12,
};

#[cfg(feature = "rand")]
pub use random::{HegelRandom, RandomsGenerator, randoms};
