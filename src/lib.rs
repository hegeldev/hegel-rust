//! Hegel is a property-based testing library for Rust. Hegel is based on [Hypothesis](https://github.com/hypothesisworks/hypothesis), using the [Hegel](https://hegel.dev/) protocol.
//!
//! # Getting started with Hegel for Rust
//!
//! This guide walks you through the basics of installing Hegel and writing your first tests.
//!
//! ## Prerequisites
//!
//! You will need [`uv`](https://docs.astral.sh/uv/) installed and on your PATH.
//!
//! ## Install Hegel
//!
//! Add `hegel-rust` to your `Cargo.toml` as a dev dependency using cargo:
//!
//! ```bash
//! cargo add --dev hegeltest
//! ```
//!
//! ## Write your first test
//!
//! You're now ready to write your first test. We'll use Cargo as a test runner for the
//! purposes of this guide. Create a new test in the project's `tests/` directory:
//!
//! ```no_run
//! use hegel::TestCase;
//! use hegel::generators as gs;
//!
//! #[hegel::test]
//! fn test_integer_self_equality(tc: TestCase) {
//!     let n = tc.draw(gs::integers::<i32>());
//!     assert_eq!(n, n); // integers should always be equal to themselves
//! }
//! ```
//!
//! Now run the test using `cargo test --test <filename>`. You should see that this test passes.
//!
//! Let's look at what's happening in more detail. The `#[hegel::test]` attribute runs your test
//! many times (100, by default). The test function (in this case `test_integer_self_equality`)
//! takes a [`TestCase`] parameter, which provides a [`draw`](TestCase::draw) method for drawing
//! different values. This test draws a random integer and checks that it should be equal to itself.
//!
//! Next, try a test that fails:
//!
//! ```no_run
//! # use hegel::TestCase;
//! # use hegel::generators as gs;
//! #[hegel::test]
//! fn test_integers_always_below_50(tc: TestCase) {
//!     let n = tc.draw(gs::integers::<i32>());
//!     assert!(n < 50); // this will fail!
//! }
//! ```
//!
//! This test asserts that any integer is less than 50, which is obviously incorrect. Hegel will
//! find a test case that makes this assertion fail, and then shrink it to find the smallest
//! counterexample — in this case, `n = 50`.
//!
//! To fix this test, you can constrain the integers you generate with the `min_value` and
//! `max_value` functions:
//!
//! ```no_run
//! # use hegel::TestCase;
//! # use hegel::generators as gs;
//! #[hegel::test]
//! fn test_bounded_integers_always_below_50(tc: TestCase) {
//!     let n = tc.draw(gs::integers::<i32>()
//!         .min_value(0)
//!         .max_value(49));
//!     assert!(n < 50);
//! }
//! ```
//!
//! Run the test again. It should now pass.
//!
//! ## Use generators
//!
//! Hegel provides a rich library of generators that you can use out of the box. There are
//! primitive generators, such as [`integers`](generators::integers),
//! [`floats`](generators::floats), and [`text`](generators::text), and combinators that allow
//! you to make generators out of other generators, such as [`vecs`](generators::vecs) and
//! `tuples`.
//!
//! For example, you can use [`vecs`](generators::vecs) to generate a vector of integers:
//!
//! ```no_run
//! # use hegel::TestCase;
//! use hegel::generators as gs;
//!
//! #[hegel::test]
//! fn test_append_increases_length(tc: TestCase) {
//!     let mut vector = tc.draw(gs::vecs(gs::integers::<i32>()));
//!     let initial_length = vector.len();
//!     vector.push(tc.draw(gs::integers::<i32>()));
//!     assert!(vector.len() > initial_length);
//! }
//! ```
//!
//! This test checks that appending an element to a random vector of integers should always
//! increase its length.
//!
//! You can also define custom generators. For example, say you have a `Person` struct that
//! we want to generate:
//!
//! ```no_run
//! # use hegel::TestCase;
//! # use hegel::generators as gs;
//! #[derive(Debug)]
//! struct Person {
//!     age: i32,
//!     name: String,
//! }
//!
//! #[hegel::composite]
//! fn generate_person(tc: TestCase) -> Person {
//!     let age = tc.draw(gs::integers::<i32>());
//!     let name = tc.draw(gs::text());
//!     Person { age, name }
//! }
//! ```
//!
//! Note that you can feed the results of a `draw` to subsequent calls. For example, say that
//! you extend the `Person` struct to include a `driving_license` boolean field:
//!
//! ```no_run
//! # use hegel::TestCase;
//! # use hegel::generators as gs;
//! #[derive(Debug)]
//! struct Person {
//!     age: i32,
//!     name: String,
//!     driving_license: bool,
//! }
//!
//! #[hegel::composite]
//! fn generate_person(tc: TestCase) -> Person {
//!     let age = tc.draw(gs::integers::<i32>());
//!     let name = tc.draw(gs::text());
//!     let driving_license = if age >= 18 {
//!         tc.draw(gs::booleans())
//!     } else {
//!          false
//!     };
//!     Person { age, name, driving_license }
//! }
//! ```
//!
//! ## Debug your failing test cases
//!
//! Use the [`note`](TestCase::note) method to attach debug information:
//!
//! ```no_run
//! # use hegel::TestCase;
//! # use hegel::generators as gs;
//! #[hegel::test]
//! fn test_with_notes(tc: TestCase) {
//!     let x = tc.draw(gs::integers::<i32>());
//!     let y = tc.draw(gs::integers::<i32>());
//!     tc.note(&format!("x + y = {}, y + x = {}", x + y, y + x));
//!     assert_eq!(x + y, y + x);
//! }
//! ```
//!
//! Notes only appear when Hegel replays the minimal failing example.
//!
//! ## Change the number of test cases
//!
//! By default Hegel runs 100 test cases. To override this, pass the `test_cases` argument
//! to the `test` attribute:
//!
//! ```no_run
//! # use hegel::TestCase;
//! # use hegel::generators as gs;
//! #[hegel::test(test_cases = 500)]
//! fn test_integers_many(tc: TestCase) {
//!     let n = tc.draw(gs::integers::<i32>());
//!     assert_eq!(n, n);
//! }
//! ```
//!
//! ## Learning more
//!
//! - Browse the [`generators`] module for the full list of available generators.
//! - See [`Settings`] for more configuration settings to customise how your test runs.

#![forbid(future_incompatible)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub(crate) mod antithesis;
pub mod backend;
pub(crate) mod cbor_utils;
pub(crate) mod control;
pub mod explicit_test_case;
pub mod generators;
#[cfg(feature = "native")]
pub(crate) mod native;
pub(crate) mod runner;
#[cfg(not(feature = "native"))]
pub(crate) mod server;
pub mod stateful;
mod test_case;

#[doc(hidden)]
pub use control::currently_in_test_context;
pub use explicit_test_case::ExplicitTestCase;
pub use generators::Generator;
pub use test_case::TestCase;

// re-export for macro use
#[doc(hidden)]
pub use ciborium;
#[doc(hidden)]
pub use paste;
#[doc(hidden)]
pub use test_case::{__IsTestCase, __assert_is_test_case, generate_from_schema, generate_raw};

// re-export public api
#[doc(hidden)]
pub use antithesis::TestLocation;

// Re-exports of native-engine internals for integration-test access.
// `#[doc(hidden)]` — not part of the stable public API.
#[cfg(feature = "native")]
#[doc(hidden)]
pub mod __native_test_internals {
    pub use crate::native::core::StringChoice;
}

/// Derive a generator for a struct or enum.
///
/// This implements [`DefaultGenerator`](generators::DefaultGenerator) for the type,
/// allowing it to be used with [`default`](generators::default) via `default::<T>()`.
///
/// For structs, the generated generator has:
/// - `<field>(generator)` - builder method to customize each field's generator
///
/// For enums, the generated generator has:
/// - `default_<VariantName>()` - methods returning default variant generators
/// - `<VariantName>(generator)` - builder methods to customize variant generation
///
/// # Struct Example
///
/// ```ignore
/// use hegel::DefaultGenerator;
/// use hegel::generators::{self as gs, DefaultGenerator as _, Generator as _};
///
/// #[derive(DefaultGenerator)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// #[hegel::test]
/// fn generates_people(tc: hegel::TestCase) {
///     let generator = gs::default::<Person>()
///         .age(gs::integers::<u32>().min_value(0).max_value(120));
///     let person: Person = tc.draw(generator);
/// }
/// ```
///
/// # Enum Example
///
/// ```ignore
/// use hegel::DefaultGenerator;
/// use hegel::generators::{self as gs, DefaultGenerator as _, Generator as _};
///
/// #[derive(DefaultGenerator)]
/// enum Status {
///     Pending,
///     Active { since: String },
///     Error { code: i32, message: String },
/// }
///
/// #[hegel::test]
/// fn generates_statuses(tc: hegel::TestCase) {
///     let generator = gs::default::<Status>()
///         .Active(
///             gs::default::<Status>()
///                 .default_Active()
///                 .since(gs::text().max_size(20))
///         );
///     let status: Status = tc.draw(generator);
/// }
/// ```
pub use hegel_macros::DefaultGenerator;

/// Define a composite generator from a function.
///
/// The first parameter must be a [`TestCase`] and is passed automatically
/// when the generator is drawn. Any additional parameters become parameters
/// of the returned factory function. The function must have an explicit
/// return type.
///
/// ```ignore
/// use hegel::generators as gs;
///
/// #[hegel::composite]
/// fn sorted_vec(tc: hegel::TestCase, min_len: usize) -> Vec<i32> {
///     let mut v: Vec<i32> = tc.draw(gs::vecs(gs::integers()).min_size(min_len));
///     v.sort();
///     v
/// }
///
/// #[hegel::test]
/// fn test_sorted(tc: hegel::TestCase) {
///     let v = tc.draw(sorted_vec(3));
///     assert!(v.len() >= 3);
///     assert!(v.windows(2).all(|w| w[0] <= w[1]));
/// }
/// ```
pub use hegel_macros::composite;
pub use hegel_macros::explicit_test_case;

/// Derive a [`StateMachine`](crate::stateful::StateMachine) implementation from an `impl` block.
///
/// See the [`stateful`] module docs for more information.
pub use hegel_macros::state_machine;

/// The main entrypoint into Hegel.
///
/// The function must take exactly one parameter of type [`TestCase`]. The test case can be
/// used to generate values via [`TestCase::draw`].
///
/// The `#[test]` attribute is added automatically and must not be present on the function.
///
/// ```ignore
/// #[hegel::test]
/// fn my_test(tc: TestCase) {
///     let x: i32 = tc.draw(integers());
///     assert!(x + 0 == x);
/// }
/// ```
///
/// You can set settings using attributes on [`test`], corresponding to methods on [`Settings`]:
///
/// ```ignore
/// #[hegel::test(test_cases = 500)]
/// fn test_runs_many_more_times(tc: TestCase) {
///     let x: i32 = tc.draw(integers());
///     assert!(x + 0 == x);
/// }
/// ```
pub use hegel_macros::test;

#[doc(hidden)]
pub use runner::hegel;
pub use runner::{HealthCheck, Hegel, Settings, Verbosity};
#[cfg(not(feature = "native"))]
#[doc(hidden)]
pub use server::process::__test_kill_server;
#[cfg(not(feature = "native"))]
#[doc(hidden)]
pub use server::process::format_log_excerpt;
