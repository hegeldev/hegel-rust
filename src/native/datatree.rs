//! Constants shared with `ChoiceKind::max_children` (in `core/choices.rs`).
//!
//! The cardinality calculation itself now lives as a method on
//! [`ChoiceKind`], so consumers say `kind.max_children()` rather than
//! threading a free function.

/// Upper bound on [`ChoiceKind::max_children`](super::core::ChoiceKind::max_children)
/// where further precision is not useful. Matches upstream's
/// `MAX_CHILDREN_EFFECTIVELY_INFINITE`.
pub const MAX_CHILDREN_EFFECTIVELY_INFINITE: u64 = 10_000_000;

#[cfg(test)]
#[path = "../../tests/embedded/native/datatree_tests.rs"]
mod tests;
