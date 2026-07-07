//! Generators for producing test data.
//!
//! Start with the [factory functions below](#functions) — each one returns a builder.
//! Most builders have methods for constraining the output (e.g. `.min_value()`, `.max_size()`).
//! All generators implement [`Generator<T>`], which provides combinators like
//! [`map`](Generator::map), [`filter`](Generator::filter), and
//! [`flat_map`](Generator::flat_map).

mod collections;
mod combinators;
mod compose;
mod default;
mod deferred;
#[allow(clippy::module_inception)]
mod generators;
mod misc;
mod numeric;
mod strings;
mod time;
mod tuples;

#[doc(hidden)]
pub use crate::test_case::{Collection, TestCase, labels};

#[doc(inline)]
pub use crate::tuples;
pub use collections::{
    ArrayGenerator, HashMapGenerator, HashSetGenerator, VecGenerator, arrays, hashmaps, hashsets,
    vecs,
};
pub use combinators::{
    OneOfGenerator, OptionalGenerator, SampledFromGenerator, one_of, optional, sampled_from,
};
pub use compose::ComposedGenerator;
#[doc(hidden)]
pub use compose::fnv1a_hash;
pub use default::{DefaultGenerator, default};
pub use deferred::{DeferredGeneratorDefinition, deferred};
pub use generators::{BoxedGenerator, Filtered, FlatMapped, Generator, Mapped};
pub use misc::{BoolGenerator, JustGenerator, booleans, just, unit};
pub use numeric::{Float, FloatGenerator, Integer, IntegerGenerator, floats, integers};
pub use strings::{
    BinaryGenerator, CharactersGenerator, DateGenerator, DateTimeGenerator, DomainGenerator,
    EmailGenerator, IpAddressGenerator, Ipv4AddressGenerator, Ipv6AddressGenerator, RegexGenerator,
    TextGenerator, TimeGenerator, UrlGenerator, UuidsGenerator, binary, characters, dates,
    datetimes, domains, emails, from_regex, ip_addresses, text, times, urls, uuids,
};
pub use time::{DurationGenerator, durations};
#[doc(hidden)]
pub use tuples::{
    tuples0, tuples1, tuples2, tuples3, tuples4, tuples5, tuples6, tuples7, tuples8, tuples9,
    tuples10, tuples11, tuples12,
};
