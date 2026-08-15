//! The items you need in almost every Hegel test, in one import.
//!
//! ```no_run
//! use hegel::prelude::*;
//! use hegel::generators as gs;
//!
//! #[composite]
//! fn even_integers(tc: &TestCase) -> i32 {
//!     tc.draw(gs::integers::<i32>().map(|n: i32| n.wrapping_mul(2)))
//! }
//!
//! #[hegel::test]
//! fn test_sum_of_even_integers_is_even(tc: TestCase) {
//!     let xs = tc.draw(gs::vecs(even_integers()));
//!     let total = xs.iter().fold(0i32, |acc, n| acc.wrapping_add(*n));
//!     assert_eq!(total % 2, 0);
//! }
//! ```
//!
//! Three of the names above come from the prelude: [`TestCase`], the
//! [`composite`](macro@crate::composite) attribute (written bare rather than
//! as `#[hegel::composite]`), and the [`Generator`] trait, which has to be in
//! scope for the [`map`](crate::generators::Generator::map) call. Without it,
//! `map` resolves against [`Iterator`] and rustc reports the generator "is not
//! an iterator". The prelude also re-exports the [`generators`] module.
//!
//! It omits [`hegel::test`](macro@crate::test), whose name would collide with
//! the standard `#[test]` attribute. Write `#[hegel::test]` on your test
//! functions, as above.
//!
//! Generators are conventionally reached through a `gs` alias
//! (`use hegel::generators as gs;`) rather than the glob-imported
//! [`generators`] name, so most tests will still want that second import.

pub use crate::TestCase;
pub use crate::composite;
pub use crate::generators::{self, Generator};
