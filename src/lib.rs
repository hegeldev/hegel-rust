//! Hegel is a property-based testing library for Rust. Hegel is based on [Hypothesis](https://github.com/hypothesisworks/hypothesis), using the [Hegel](https://hegel.dev/) protocol.
//!
//! # Getting started
//!
//! This guide walks you through the basics of installing Hegel and writing your first tests.
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
//! [`tuples`].
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
//! ## Threading
//!
//! [`TestCase`] is `Send` but not `Sync`: you can clone it and move the clone
//! to another thread to drive generation from there.
//!
//! ```no_run
//! use hegel::TestCase;
//! use hegel::generators as gs;
//!
//! #[hegel::test]
//! fn test_with_worker_thread(tc: TestCase) {
//!     let tc_worker = tc.clone();
//!     let handle = std::thread::spawn(move || {
//!         tc_worker.draw(gs::vecs(gs::integers::<i32>()).max_size(10))
//!     });
//!     let xs = handle.join().unwrap();
//!     let more: bool = tc.draw(gs::booleans());
//!     let _ = (xs, more);
//! }
//! ```
//!
//! Clones share the test case's *outcome* — the whole family passes, fails,
//! or is rejected as one test case — but each clone draws from its own
//! independent, deterministic stream of choices, so several threads can
//! generate concurrently without perturbing each other's values and the
//! same seed replays the same values on every stream.
//!
//! Determinism extends only as far as your own code's determinism: if your
//! threads race on shared state, Hegel replays each stream faithfully but
//! the test may still behave differently run to run — see [`TestCase`]'s
//! documentation for the full contract and the patterns that are safe to
//! rely on.
//!
//! ## Learning more
//!
//! - Browse the [`generators`] module for the full list of available generators.
//! - See [`Settings`] for more configuration settings to customise how your test runs.

#![forbid(future_incompatible)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub(crate) mod antithesis;
#[doc(hidden)]
pub mod backend;
pub(crate) mod cli;
pub(crate) mod control;
#[doc(hidden)]
pub mod explicit_test_case;
pub mod extras;
pub(crate) mod ffi;
pub mod generators;
pub mod pretty;
#[doc(hidden)]
pub mod run_lifecycle;
pub(crate) mod runner;
pub mod stateful;
mod test_case;
#[doc(hidden)]
pub use control::currently_in_test_context;
pub use explicit_test_case::ExplicitTestCase;
pub use generators::Generator;
pub use generators::PrintableGenerator;
pub use pretty::{DeferredPrinter, PrettyPrintable, PrettyPrinter};
pub use test_case::TestCase;

#[doc(hidden)]
pub use paste;
#[doc(hidden)]
pub use test_case::{__IsTestCase, __assert_is_test_case, with_output_override};

#[doc(hidden)]
pub use antithesis::TestLocation;

#[doc(hidden)]
#[cfg(feature = "__bench")]
pub use hegel_c::__bench;

/// Derive a generator for a struct or enum.
///
/// This implements [`DefaultGenerator`](generators::DefaultGenerator) for the type,
/// allowing it to be used with [`default`](generators::default) via `default::<T>()`.
///
/// The derived generator prints values field by field as it draws them, in
/// the same Rust-expression format `#[derive(PrettyPrintable)]` produces,
/// so the type itself needs no [`PrettyPrintable`] implementation. It is
/// generic over its field generators — mirroring `one_of!` and tuples — and
/// is a [`PrintableGenerator`] exactly when every field generator is one:
/// the builder methods accept any [`Generator`] of the field's type, and a
/// non-printable field generator simply makes the result silent-only (or
/// printable again via [`print_as_value`](generators::Generator::print_as_value),
/// [`print_as_debug`](generators::Generator::print_as_debug), or
/// [`print_with`](generators::Generator::print_with)). A type that wants a
/// different printed representation implements [`DefaultGenerator`] by
/// hand.
///
/// For structs, the generated generator has:
/// - `<field>(generator)` - builder method to customize each field's generator
///
/// For enums, the generated generator draws one of the variants at random.
/// Unit variants need no configuration; every data-carrying variant gets
/// builder methods named after the variant (snake_cased):
/// - for a struct variant like `Active { since: String }`, a method
///   `.active(|g| ...)` whose closure receives that variant's generator
///   (with a `<field>(generator)` builder per field, like a struct) and
///   returns the generator to use for the variant;
/// - for a tuple variant like `Error(i32, String)`, a method
///   `.error(g0, g1)` taking one generator per field positionally, plus a
///   closure form `.error_with(|g| ...)` mirroring the struct-variant
///   method, where the variant generator's fields are configured with
///   `._0(...)`, `._1(...)`, etc.
///
/// # Struct Example
///
/// ```no_run
/// use hegel::DefaultGenerator;
/// use hegel::generators as gs;
///
/// #[derive(Debug, DefaultGenerator)]
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
/// ```no_run
/// use hegel::DefaultGenerator;
/// use hegel::generators as gs;
///
/// #[derive(Debug, DefaultGenerator)]
/// enum Status {
///     Pending,
///     Active { since: String },
///     Error(i32, String),
/// }
///
/// #[hegel::test]
/// fn generates_statuses(tc: hegel::TestCase) {
///     let generator = gs::default::<Status>()
///         // Struct variant: configure through a closure over the
///         // variant's own generator.
///         .active(|g| g.since(gs::text().max_size(20)))
///         // Tuple variant: pass one generator per field...
///         .error(gs::integers::<i32>().min_value(400).max_value(599), gs::text())
///         // ...or use the closure form with positional field builders.
///         .error_with(|g| g._0(gs::just(500)));
///     let status: Status = tc.draw(generator);
/// }
/// ```
pub use hegel_macros::DefaultGenerator;

/// Derive [`PrettyPrintable`] for a struct or enum.
///
/// The generated implementation prints the value in Rust-expression syntax —
/// `Name { field: value, … }`, `Name(value, …)`, and `Name::Variant …` for
/// enums — using the printer's group machinery so values that do not fit on
/// one line wrap with each field on its own line. Every generic type
/// parameter is given a [`PrettyPrintable`] bound, mirroring how
/// `derive(Debug)` bounds `Debug`.
///
/// For a type whose `Debug` output is already the representation you want
/// (or one you cannot add a derive to), use
/// [`pretty_print_as_debug!`](crate::pretty_print_as_debug) instead.
///
/// A field whose type cannot implement [`PrettyPrintable`] — a foreign type
/// the orphan rule keeps out, say — can opt out with `#[pretty(debug)]`:
/// that field prints its `Debug` representation (re-laid-out through the
/// printer, like [`print_as_debug`](generators::Generator::print_as_debug)),
/// and its type must implement `Debug` instead.
///
/// ```
/// use hegel::{PrettyPrintable, PrettyPrinter};
///
/// #[derive(PrettyPrintable)]
/// struct Person {
///     name: String,
///     age: u32,
///     #[pretty(debug)]
///     home: std::path::PathBuf,
/// }
///
/// let person = Person {
///     name: "Ada".to_string(),
///     age: 36,
///     home: "/home/ada".into(),
/// };
/// let mut printer = PrettyPrinter::new(79);
/// person.pretty_print(&mut printer);
/// assert_eq!(
///     printer.value(),
///     "Person { name: \"Ada\".to_string(), age: 36, home: \"/home/ada\" }"
/// );
/// ```
pub use hegel_macros::PrettyPrintable;

/// Define a composite generator from a function.
///
/// The first parameter must be a [`TestCase`] and is passed automatically
/// when the generator is drawn. Any additional parameters become parameters
/// of the returned factory function. The function must have an explicit
/// return type.
///
/// ```no_run
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

/// Replay a single failing example from a base64 *failure blob*.
///
/// When a test fails on the native backend and the
/// [`print_blob`](Settings::print_blob) setting is enabled, Hegel prints a
/// reproducer line of the form:
///
/// ```text
/// To reproduce this failure, add the attribute below #[hegel::test]:
///     #[hegel::reproduce_failure("AAEC…")]
/// ```
///
/// Paste that attribute **below** `#[hegel::test]` and the next run will
/// decode the blob's choice sequence and run *only* that example.
///
/// ```no_run
/// #[hegel::test]
/// #[hegel::reproduce_failure("AAEC…")]
/// fn my_test(tc: hegel::TestCase) {
///     let x: i32 = tc.draw(hegel::generators::integers());
///     assert!(x < 100);
/// }
/// ```
///
/// The argument is any expression that resolves to a base64 blob — a string
/// literal, or a `const`/`static`/variable holding one:
///
/// ```no_run
/// const REGRESSION: &str = "AAEC…";
///
/// #[hegel::test]
/// #[hegel::reproduce_failure(REGRESSION)]
/// fn my_test(tc: hegel::TestCase) { /* ... */ }
/// ```
///
/// The attribute may be stacked to keep track of several failures, but only
/// the **first** one replays — the rest are bookkeeping. Delete them one by
/// one as the failures are fixed:
///
/// ```no_run
/// #[hegel::test]
/// #[hegel::reproduce_failure("AAEC…")] // replayed
/// #[hegel::reproduce_failure("AAED…")] // kept for later
/// fn my_test(tc: hegel::TestCase) { /* ... */ }
/// ```
///
/// The blob encodes Hegel's internal choice sequence, so it is only
/// guaranteed to reproduce a failure within a specific version of Hegel.
/// A blob that can't be decoded (corrupt or from an incompatible version),
/// or that no longer reproduces a failure, panics with an explanatory
/// message.
pub use hegel_macros::reproduce_failure;

#[doc(hidden)]
pub use hegel_macros::rewrite_draws;

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
/// ```no_run
/// use hegel::TestCase;
/// use hegel::generators::integers;
///
/// #[hegel::test]
/// fn my_test(tc: TestCase) {
///     let x: i32 = tc.draw(integers());
///     assert!(x + 0 == x);
/// }
/// ```
///
/// You can set settings using attributes on [`test`], corresponding to methods on [`Settings`]:
///
/// ```no_run
/// use hegel::TestCase;
/// use hegel::generators::integers;
///
/// #[hegel::test(test_cases = 500)]
/// fn test_runs_many_more_times(tc: TestCase) {
///     let x: i32 = tc.draw(integers());
///     assert!(x + 0 == x);
/// }
/// ```
///
/// You can use other test attribute macros, like `tokio::test`, by putting them *before* `hegel::test`:
///
/// ```no_run
/// #[tokio::test]
/// #[hegel::test]
/// async fn my_async_test(tc: hegel::TestCase) {
///     let x: bool = tc.draw(hegel::generators::booleans());
///     let handle = tokio::spawn(async move { x });
///     assert_eq!(handle.await.unwrap(), x);
/// }
/// ```
pub use hegel_macros::test;

/// Turn a function into a standalone Hegel binary entry point.
///
/// The function must take exactly one parameter of type [`TestCase`]. Behaves
/// like [`test`] — draws are rewritten to record variable names, and any
/// `#[hegel::explicit_test_case]` attributes are run first — but instead of
/// producing a `#[test]` it produces a plain function body that parses CLI
/// arguments and runs a [`Hegel`] driver.
///
/// Supported CLI flags (with defaults taken from the attribute args):
/// `--test-cases`, `--seed`, `--verbosity`, `--derandomize`, `--database`,
/// `--suppress-health-check`, `-h` / `--help`.
///
/// ```no_run
/// use hegel::TestCase;
/// use hegel::generators as gs;
///
/// #[hegel::main(test_cases = 500)]
/// fn main(tc: TestCase) {
///     let n: i32 = tc.draw(gs::integers());
///     assert_eq!(n + 0, n);
/// }
/// ```
pub use hegel_macros::main;

/// Rewrite a function taking a [`TestCase`] plus additional arguments into
/// one that takes just those arguments and internally runs Hegel.
///
/// Behaves like [`test`] for name rewriting, explicit test cases, and
/// settings parsing. The generated function has the original signature
/// with the `TestCase` parameter removed, and its body is run as an
/// [`FnMut`] closure inside [`Hegel::run`].
///
/// ```no_run
/// use hegel::TestCase;
/// use hegel::generators as gs;
///
/// #[hegel::standalone_function(test_cases = 10)]
/// fn check_addition_commutative(tc: TestCase, increment: i32) {
///     let n: i32 = tc.draw(gs::integers());
///     assert_eq!(n + increment, increment + n);
/// }
///
/// // callers invoke it as a normal function:
/// # fn _example() {
/// check_addition_commutative(5);
/// # }
/// ```
pub use hegel_macros::standalone_function;

#[doc(hidden)]
pub use cli::CliOutcome;
#[doc(hidden)]
pub use cli::apply_cli_args as __apply_cli_args;
#[doc(hidden)]
pub use runner::hegel;
pub use runner::{Backend, HealthCheck, Hegel, Mode, Phase, Settings, Verbosity};
