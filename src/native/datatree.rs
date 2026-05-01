//! Ports of primitives from `hypothesis.internal.conjecture.datatree`.
//!
//! Currently limited to [`compute_max_children`], which returns the
//! cardinality of the choice space described by a given [`ChoiceKind`].
//! The DataTree itself is not yet ported.

use super::core::ChoiceKind;
use super::floats::count_between_floats;
use crate::native::bignum::BigUint;

/// Upper bound on [`compute_max_children`] where further precision is not
/// useful. Matches upstream's `MAX_CHILDREN_EFFECTIVELY_INFINITE`.
pub const MAX_CHILDREN_EFFECTIVELY_INFINITE: u64 = 10_000_000;

/// Count of distinct sequences over an alphabet of `alphabet_size` with
/// length in `[min_size, max_size]`, capped at
/// [`MAX_CHILDREN_EFFECTIVELY_INFINITE`].
///
/// Port of `_count_distinct_strings` from upstream's `datatree.py`.
fn count_distinct_strings(alphabet_size: u64, min_size: usize, max_size: usize) -> BigUint {
    if alphabet_size == 0 {
        // Only the empty string is valid.
        return BigUint::from(1u32);
    }
    if alphabet_size == 1 {
        return BigUint::from((max_size - min_size + 1) as u64);
    }

    let cap = BigUint::from(MAX_CHILDREN_EFFECTIVELY_INFINITE);
    let alpha = BigUint::from(alphabet_size);
    let mut total = BigUint::from(0u32);
    for length in min_size..=max_size {
        total += alpha.pow(length as u32);
        if total >= cap {
            return cap;
        }
    }
    total
}

/// Cardinality of the choice space described by `kind`. Port of
/// `compute_max_children` from upstream's `datatree.py`, adapted to hegel's
/// [`ChoiceKind`] representation.
///
/// Differs from upstream in two ways that follow from hegel's simpler
/// native shapes:
///
/// * [`BooleanChoice`] carries no `p` parameter, so this always returns 2
///   for booleans (upstream collapses to 1 when `p` is effectively 0 or 1).
/// * [`FloatChoice`] carries no `smallest_nonzero_magnitude`, so the
///   float count is just [`count_between_floats`] over the range.
///
/// [`BooleanChoice`]: super::core::BooleanChoice
/// [`FloatChoice`]: super::core::FloatChoice
pub fn compute_max_children(kind: &ChoiceKind) -> BigUint {
    match kind {
        ChoiceKind::Integer(ic) => {
            // max_value - min_value + 1 without i128 overflow when the
            // range spans the full i128 space.
            let diff = (ic.max_value as u128).wrapping_sub(ic.min_value as u128);
            BigUint::from(diff) + BigUint::from(1u32)
        }
        ChoiceKind::Boolean(_) => BigUint::from(2u32),
        ChoiceKind::Float(fc) => BigUint::from(count_between_floats(fc.min_value, fc.max_value)),
        ChoiceKind::Bytes(bc) => count_distinct_strings(256, bc.min_size, bc.max_size),
        ChoiceKind::String(sc) => count_distinct_strings(sc.alpha_size(), sc.min_size, sc.max_size),
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/datatree_tests.rs"]
mod tests;
