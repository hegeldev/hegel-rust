//! The items you need in almost every Hegel test, in one import.
//!
//! ```no_run
//! use hegel::prelude::*;
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
//! Every name in that test comes from the prelude: [`TestCase`], the
//! [`composite`](macro@crate::composite) attribute (written bare rather than
//! as `#[hegel::composite]`), the [`Generator`] trait, which has to be in
//! scope for the [`map`](crate::generators::Generator::map) call, and `gs`.
//! Without `Generator`, `map` resolves against [`Iterator`] instead and rustc
//! reports that your generator "is not an iterator".
//!
//! `gs` is the conventional alias for the [`generators`] module, which the
//! prelude re-exports under both names. They are the same module, so
//! `gs::integers()` and `generators::integers()` are interchangeable; the
//! short form is the one the rest of this documentation uses. If you bind
//! `gs` to something of your own, your explicit import shadows the prelude's
//! without an ambiguity error.
//!
//! The prelude omits [`hegel::test`](macro@crate::test), whose name would
//! collide with the standard `#[test]` attribute. Write `#[hegel::test]` on
//! your test functions, as above.

pub use crate::TestCase;
pub use crate::composite;
pub use crate::generators as gs;
pub use crate::generators::{self, Generator};
