//! Hegel is a property-based testing framework for Rust.
//!
//! # Quick Start
//!
//! ```no_run
//! use hegel::generators;
//!
//! #[test]
//! fn test_addition_commutative() {
//!     hegel::hegel(|| {
//!         let x = hegel::draw(&generators::integers::<i32>());
//!         let y = hegel::draw(&generators::integers::<i32>());
//!         assert_eq!(x + y, y + x);
//!     });
//! }
//! ```
//!
//! # Configuration
//!
//! Use the builder pattern for more control:
//!
//! ```no_run
//! use hegel::{Hegel, Verbosity};
//! use hegel::generators;
//!
//! #[test]
//! fn test_with_options() {
//!     Hegel::new(|| {
//!         let n = hegel::draw(&generators::integers::<i32>());
//!         assert!(n + 0 == n);
//!     })
//!     .test_cases(500)
//!     .verbosity(Verbosity::Verbose)
//!     .run();
//! }
//! ```
//!
//! # Generators
//!
//! All generators implement [`generators::Generate<T>`] and are created via factory functions
//! in the [`generators`] module.
//!
//! ## Primitives
//!
//! ```no_run
//! use hegel::generators;
//!
//! # hegel::hegel(|| {
//! let _: () = hegel::draw(&generators::unit());
//! let b: bool = hegel::draw(&generators::booleans());
//! let n: i32 = hegel::draw(&generators::just(42));  // constant with schema
//! # });
//! ```
//!
//! ## Numbers
//!
//! ```no_run
//! use hegel::generators;
//!
//! # hegel::hegel(|| {
//! // Integers - bounds default to type limits
//! let i: i32 = hegel::draw(&generators::integers::<i32>());
//! let bounded: i32 = hegel::draw(&generators::integers().with_min(0).with_max(100));
//!
//! // Floating point
//! let f: f64 = hegel::draw(&generators::floats::<f64>());
//! let bounded: f64 = hegel::draw(&generators::floats()
//!     .with_min(0.0)
//!     .with_max(1.0)
//!     .exclude_min()
//!     .exclude_max());
//! # });
//! ```
//!
//! ## Strings
//!
//! ```no_run
//! use hegel::generators;
//!
//! # hegel::hegel(|| {
//! let s: String = hegel::draw(&generators::text());
//! let bounded: String = hegel::draw(&generators::text().with_min_size(1).with_max_size(100));
//!
//! // Regex patterns (auto-anchored)
//! let pattern: String = hegel::draw(&generators::from_regex(r"[a-z]{3}-[0-9]{3}"));
//!
//! // Format strings
//! let email: String = hegel::draw(&generators::emails());
//! let url: String = hegel::draw(&generators::urls());
//! let ip: String = hegel::draw(&generators::ip_addresses().v4());
//! let date: String = hegel::draw(&generators::dates());  // YYYY-MM-DD
//! # });
//! ```
//!
//! ## Collections
//!
//! ```no_run
//! use hegel::generators;
//! use std::collections::{HashSet, HashMap};
//!
//! # hegel::hegel(|| {
//! let vec: Vec<i32> = hegel::draw(&generators::vecs(generators::integers()).with_min_size(1));
//! let set: HashSet<i32> = hegel::draw(&generators::hashsets(generators::integers()));
//! let map: HashMap<String, i32> = hegel::draw(&generators::hashmaps(generators::text(), generators::integers()));
//! # });
//! ```
//!
//! ## Combinators
//!
//! ```no_run
//! use hegel::generators;
//!
//! # hegel::hegel(|| {
//! // Sample from a fixed set
//! let color: &str = hegel::draw(&generators::sampled_from(vec!["red", "green", "blue"]));
//!
//! // Choose from multiple generators
//! let n: i32 = hegel::draw(&hegel::one_of!(
//!     generators::integers::<i32>().with_min(0).with_max(10),
//!     generators::integers::<i32>().with_min(100).with_max(110),
//! ));
//!
//! // Optional values
//! let opt: Option<i32> = hegel::draw(&generators::optional(generators::integers()));
//! # });
//! ```
//!
//! ## Transformations
//!
//! ```no_run
//! use hegel::generators::{self, Generate};
//!
//! # hegel::hegel(|| {
//! // Transform values
//! let squared: i32 = hegel::draw(&generators::integers::<i32>()
//!     .with_min(1)
//!     .with_max(10)
//!     .map(|x| x * x));
//!
//! // Filter values
//! let even: i32 = hegel::draw(&generators::integers::<i32>()
//!     .filter(|x| x % 2 == 0));
//!
//! // Dependent generation
//! let sized: String = hegel::draw(&generators::integers::<usize>()
//!     .with_min(1)
//!     .with_max(10)
//!     .flat_map(|len| generators::text().with_min_size(len).with_max_size(len)));
//! # });
//! ```
//!
//! # Deriving Generators
//!
//! Use `#[derive(Generate)]` to automatically create generators for structs and enums,
//! then use [`generators::from_type`] to get a generator:
//!
//! ```no_run
//! use hegel::Generate;
//! use hegel::generators;
//!
//! #[derive(Generate, Debug)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! # hegel::hegel(|| {
//! // Generate with defaults
//! let person: Person = hegel::draw(&generators::from_type::<Person>());
//!
//! // Customize field generators
//! let person: Person = hegel::draw(&generators::from_type::<Person>()
//!     .with_age(generators::integers().with_min(0).with_max(120)));
//! # });
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
//! let point: Point = hegel::draw(&generators::from_type::<Point>());
//! ```
//!
//! # Assumptions
//!
//! Use [`assume`] to reject invalid test inputs:
//!
//! ```no_run
//! use hegel::generators;
//!
//! # hegel::hegel(|| {
//! let age: u32 = hegel::draw(&generators::integers());
//! hegel::assume(age >= 18);
//! // Test logic for adults only...
//! # });
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
pub mod control;
pub mod generators;
pub mod stateful;
pub(crate) mod protocol;
pub(crate) mod runner;

pub use control::{assume, currently_in_test_context, draw, note};
pub use generators::Generate;

// Re-export for macro use
#[doc(hidden)]
pub use ciborium;
#[doc(hidden)]
pub use paste;

// re-export public api
pub use hegel_derive::test;
pub use hegel_derive::Generate;
pub use runner::{hegel, Hegel, Verbosity};
