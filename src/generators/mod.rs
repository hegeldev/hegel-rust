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
mod recursive;
mod sequences;
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
    OneOf1Generator, OneOf2Generator, OneOf3Generator, OneOf4Generator, OneOf5Generator,
    OneOf6Generator, OneOf7Generator, OneOf8Generator, OneOf9Generator, OneOf10Generator,
    OneOf11Generator, OneOf12Generator, OneOf13Generator, OneOf14Generator, OneOf15Generator,
    OneOf16Generator, OneOf17Generator, OneOf18Generator, OneOf19Generator, OneOf20Generator,
    OneOf21Generator, OneOf22Generator, OneOf23Generator, OneOf24Generator, OneOf25Generator,
    OneOf26Generator, OneOf27Generator, OneOf28Generator, OneOf29Generator, OneOf30Generator,
    OneOfGenerator, OptionalGenerator, SampledFromGenerator, draw_one_of, one_of, optional,
    sampled_from,
};
#[doc(hidden)]
pub use combinators::{
    one_of1, one_of2, one_of3, one_of4, one_of5, one_of6, one_of7, one_of8, one_of9, one_of10,
    one_of11, one_of12, one_of13, one_of14, one_of15, one_of16, one_of17, one_of18, one_of19,
    one_of20, one_of21, one_of22, one_of23, one_of24, one_of25, one_of26, one_of27, one_of28,
    one_of29, one_of30,
};
pub use compose::ComposedGenerator;
#[doc(hidden)]
pub use compose::fnv1a_hash;
pub use default::{DefaultGenerator, default};
pub use deferred::{DeferredGeneratorDefinition, deferred, deferred_silent};
pub(crate) use generators::draw_and_print_value;
pub use generators::{
    BoxedGenerator, BoxedPrintableGenerator, Filtered, FlatMapped, Generator, Mapped,
    PrintableGenerator, PrintedAsDebug, PrintedAsValue, PrintedWith,
};
pub use misc::{BoolGenerator, JustGenerator, booleans, just, unit, weighted_booleans};
pub use numeric::{Float, FloatGenerator, Integer, IntegerGenerator, floats, integers};
pub use recursive::{RecursiveGenerator, SubtreeGenerator, recursive};
pub use sequences::{
    PermutationGenerator, SampleGenerator, SubsequenceGenerator, permutations, samples,
    subsequences,
};
pub use strings::{
    BinaryGenerator, CharactersGenerator, DateStringGenerator, DateTimeStringGenerator,
    DomainGenerator, EmailGenerator, IpAddressGenerator, Ipv4AddressGenerator,
    Ipv6AddressGenerator, RegexGenerator, TextGenerator, TimeStringGenerator, UrlGenerator,
    UuidsGenerator, binary, characters, date_strings, datetime_strings, domains, emails,
    from_regex, ip_addresses, text, time_strings, urls, uuids,
};
pub use time::{DurationGenerator, durations};
pub use tuples::{
    Tuple0Generator, Tuple1Generator, Tuple2Generator, Tuple3Generator, Tuple4Generator,
    Tuple5Generator, Tuple6Generator, Tuple7Generator, Tuple8Generator, Tuple9Generator,
    Tuple10Generator, Tuple11Generator, Tuple12Generator,
};
#[doc(hidden)]
pub use tuples::{
    tuples0, tuples1, tuples2, tuples3, tuples4, tuples5, tuples6, tuples7, tuples8, tuples9,
    tuples10, tuples11, tuples12,
};
