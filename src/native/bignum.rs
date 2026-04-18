//! Arbitrary-precision integer re-exports for the native engine.
//!
//! pbtkit leans on Python's bignum `int` in its shortlex index arithmetic
//! (`to_index` / `from_index` / `max_index` in `core.py`, `text.py`,
//! `bytes.py`). The index space for realistic choice bounds — e.g.
//! `BytesChoice { max_size: 16 }` (≈ 2¹²⁸) or a `StringChoice` spanning a
//! wide codepoint range — exceeds `u128` immediately, so the port cannot
//! use fixed-width integers here.
//!
//! Routing all big-integer arithmetic through this module keeps the
//! backend choice localised: swapping `num-bigint` for e.g. `malachite`
//! later would only touch this file.

pub use num_bigint::BigUint;
pub use num_traits::Zero;
