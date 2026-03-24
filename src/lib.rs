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
//! Add `hegel-rust` to your `Cargo.toml` as a dev dependency:
//!
//! ```toml
//! [dev-dependencies]
//! hegeltest = "0.1.0"
//! ```
//!
//! ## Write your first test
//!
//! You're now ready to write your first test. We'll use Cargo as a test runner for the
//! purposes of this guide. Create a new test in the project's `tests/` directory:
//!
//! ```no_run
//! use hegel::TestCase;
//! use hegel::generators::integers;
//!
//! #[hegel::test]
//! fn test_integer_self_equality(tc: TestCase) {
//!     let n = tc.draw(integers::<i32>());
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
//! # use hegel::generators::integers;
//! #[hegel::test]
//! fn test_integers_always_below_50(tc: TestCase) {
//!     let n = tc.draw(integers::<i32>());
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
//! # use hegel::generators::integers;
//! #[hegel::test]
//! fn test_bounded_integers_always_below_50(tc: TestCase) {
//!     let n = tc.draw(integers::<i32>()
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
//! use hegel::generators::{integers, vecs};
//!
//! #[hegel::test]
//! fn test_append_increases_length(tc: TestCase) {
//!     let mut vector = tc.draw(vecs(integers::<i32>()));
//!     let initial_length = vector.len();
//!     vector.push(tc.draw(integers::<i32>()));
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
//! # use hegel::generators::{integers, text};
//! #[derive(Debug)]
//! struct Person {
//!     age: i32,
//!     name: String,
//! }
//!
//! #[hegel::composite]
//! fn generate_person(tc: TestCase) -> Person {
//!     let age = tc.draw(integers::<i32>());
//!     let name = tc.draw(text());
//!     Person { age, name }
//! }
//! ```
//!
//! Note that you can feed the results of a `draw` to subsequent calls. For example, say that
//! you extend the `Person` struct to include a `driving_license` boolean field:
//!
//! ```no_run
//! # use hegel::TestCase;
//! # use hegel::generators::{integers, text, booleans};
//! #[derive(Debug)]
//! struct Person {
//!     age: i32,
//!     name: String,
//!     driving_license: bool,
//! }
//!
//! #[hegel::composite]
//! fn generate_person(tc: TestCase) -> Person {
//!     let age = tc.draw(integers::<i32>());
//!     let name = tc.draw(text());
//!     let driving_license = if age >= 18 {
//!         tc.draw(booleans())
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
//! # use hegel::generators::integers;
//! #[hegel::test]
//! fn test_with_notes(tc: TestCase) {
//!     let x = tc.draw(integers::<i32>());
//!     let y = tc.draw(integers::<i32>());
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
//! # use hegel::generators::integers;
//! #[hegel::test(test_cases = 500)]
//! fn test_integers_many(tc: TestCase) {
//!     let n = tc.draw(integers::<i32>());
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
pub(crate) mod cbor_utils;
pub(crate) mod control;
pub mod generators;
pub(crate) mod protocol;
pub(crate) mod runner;
pub mod stateful;
mod test_case;

#[doc(hidden)]
pub use control::currently_in_test_context;
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
pub use hegel_macros::DefaultGenerator;
pub use hegel_macros::composite;
pub use hegel_macros::state_machine;
pub use hegel_macros::test;
#[doc(hidden)]
pub use runner::hegel;
pub use runner::{HealthCheck, Hegel, Settings, Verbosity2};
