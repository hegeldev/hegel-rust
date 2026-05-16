//! Arbitrary-precision integer re-exports for the native engine.
//!
//! The shortlex index arithmetic on `IntegerChoice`
//! (`to_index` / `from_index` / `max_index`) accumulates beyond
//! fixed-width arithmetic over the full `u128`-wide index space, so
//! Hypothesis (which uses Python `int` throughout) needs a Rust
//! analogue.  Routing all big-integer arithmetic through this module
//! keeps the backend choice localised: swapping `num-bigint` for e.g.
//! `malachite` later would only touch this file.

pub use num_bigint::BigUint;
pub use num_traits::Zero;
