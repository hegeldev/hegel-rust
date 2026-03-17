//! Hegel is a property-based testing framework for Rust.
//!
//! # Quick Start
//!
//! ```no_run
//! use hegel::generators;
//!
//! #[hegel::test]
//! fn test_addition_commutative(tc: hegel::TestCase) {
//!     let x = tc.draw(generators::integers::<i32>());
//!     let y = tc.draw(generators::integers::<i32>());
//!     assert_eq!(x + y, y + x);
//! }
//! ```
//!
//! # Configuration
//!
//! Use attributes for more control:
//!
//! ```no_run
//! use hegel::Verbosity;
//! use hegel::generators;
//!
//! #[hegel::test(test_cases = 500, verbosity = Verbosity::Verbose)]
//! fn test_with_options(tc: hegel::TestCase) {
//!     let n = tc.draw(generators::integers::<i32>());
//!     assert!(n + 0 == n);
//! }
//! ```
//!
//! # Generators
//!
//! All generators implement [`generators::Generator<T>`] and are created via factory functions
//! in the [`generators`] module.
//!
//! ## Primitives
//!
//! ```no_run
//! use hegel::generators;
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     let _: () = tc.draw(generators::unit());
//!     let b: bool = tc.draw(generators::booleans());
//!     let n: i32 = tc.draw(generators::just(42));  // constant with schema
//! }
//! ```
//!
//! ## Numbers
//!
//! ```no_run
//! use hegel::generators;
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     // Integers - bounds default to type limits
//!     let i: i32 = tc.draw(generators::integers::<i32>());
//!     let bounded: i32 = tc.draw(generators::integers().min_value(0).max_value(100));
//!
//!     // Floating point
//!     let f: f64 = tc.draw(generators::floats::<f64>());
//!     let bounded: f64 = tc.draw(generators::floats()
//!         .min_value(0.0)
//!         .max_value(1.0)
//!         .exclude_min()
//!         .exclude_max());
//! }
//! ```
//!
//! ## Strings
//!
//! ```no_run
//! use hegel::generators;
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     let s: String = tc.draw(generators::text());
//!     let bounded: String = tc.draw(generators::text().min_size(1).max_size(100));
//!
//!     // Regex patterns (auto-anchored)
//!     let pattern: String = tc.draw(generators::from_regex(r"[a-z]{3}-[0-9]{3}"));
//!
//!     // Format strings
//!     let email: String = tc.draw(generators::emails());
//!     let url: String = tc.draw(generators::urls());
//!     let ip: String = tc.draw(generators::ip_addresses().v4());
//!     let date: String = tc.draw(generators::dates());  // YYYY-MM-DD
//! }
//! ```
//!
//! ## Collections
//!
//! ```no_run
//! use hegel::generators;
//! use std::collections::{HashSet, HashMap};
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     let vec: Vec<i32> = tc.draw(generators::vecs(generators::integers()).min_size(1));
//!     let set: HashSet<i32> = tc.draw(generators::hashsets(generators::integers()));
//!     let map: HashMap<String, i32> = tc.draw(generators::hashmaps(generators::text(), generators::integers()));
//! }
//! ```
//!
//! ## Combinators
//!
//! ```no_run
//! use hegel::generators;
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     // Sample from a fixed set
//!     let color: &str = tc.draw(generators::sampled_from(vec!["red", "green", "blue"]));
//!
//!     // Choose from multiple generators
//!     let n: i32 = tc.draw(hegel::one_of!(
//!         generators::integers::<i32>().min_value(0).max_value(10),
//!         generators::integers::<i32>().min_value(100).max_value(110),
//!     ));
//!
//!     // Optional values
//!     let opt: Option<i32> = tc.draw(generators::optional(generators::integers()));
//! }
//! ```
//!
//! ## Transformations
//!
//! ```no_run
//! use hegel::generators::{self, Generator};
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     // Transform values
//!     let squared: i32 = tc.draw(generators::integers::<i32>()
//!         .min_value(1)
//!         .max_value(10)
//!         .map(|x| x * x));
//!
//!     // Filter values
//!     let even: i32 = tc.draw(generators::integers::<i32>()
//!         .filter(|x| x % 2 == 0));
//!
//!     // Dependent generation
//!     let sized: String = tc.draw(generators::integers::<usize>()
//!         .min_value(1)
//!         .max_value(10)
//!         .flat_map(|len| generators::text().min_size(len).max_size(len)));
//! }
//! ```
//!
//! # Deriving Generators
//!
//! Use `#[derive(Generator)]` to automatically create generators for structs and enums,
//! then use [`generators::default`] to get a generator:
//!
//! ```no_run
//! use hegel::Generator;
//! use hegel::generators;
//!
//! #[derive(Generator, Debug)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     // Generate with defaults
//!     let person: Person = tc.draw(generators::default::<Person>());
//!
//!     // Customize field generators
//!     let person: Person = tc.draw(generators::default::<Person>()
//!         .with_age(generators::integers().min_value(0).max_value(120)));
//! }
//! ```
//!
//! For external types, use [`derive_generator!`]:
//!
//! ```ignore
//! use hegel::derive_generator;
//! use hegel::generators;
//!
//! derive_generator!(Point { x: f64, y: f64 });
//!
//! let point: Point = tc.draw(generators::default::<Point>());
//! ```
//!
//! # Assumptions
//!
//! Use [`TestCase::assume`] to reject invalid test inputs:
//!
//! ```no_run
//! use hegel::generators;
//!
//! #[hegel::test]
//! fn my_test(tc: hegel::TestCase) {
//!     let age: u32 = tc.draw(generators::integers());
//!     tc.assume(age >= 18);
//!     // Test logic for adults only...
//! }
//! ```
//!
//! # Feature Flags
//!
//! - **`rand`**: Enables [`generators::randoms()`] for generating random number generators
//!   that implement [`rand::RngCore`].
//!
//! # Debugging
//!
//! Set verbosity to [`Verbosity::Debug`] to enable debug logging of requests/responses.

pub(crate) mod cbor_utils;
pub(crate) mod control;
pub mod generators;
pub(crate) mod protocol;
pub(crate) mod runner;
mod test_case;

pub use control::currently_in_test_context;
pub use generators::Generator;
pub use test_case::TestCase;

// re-export for macro use
#[doc(hidden)]
pub use ciborium;
#[doc(hidden)]
pub use paste;
#[doc(hidden)]
pub use test_case::{generate_from_schema, generate_raw};

// re-export public api
pub use hegel_macros::Generator;
pub use hegel_macros::composite;
pub use hegel_macros::test;
pub use runner::{Hegel, Verbosity, hegel};
