const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

const fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash
}

/// The span label for a generator identified by a name.
///
/// A label is an opaque value identifying a generator to the engine, which
/// treats two spans with the same label as coming from the same generator
/// (see [`Generator::label`](super::Generator::label)). This derives one
/// from a name — the 64-bit FNV-1a hash of its UTF-8 bytes, the same
/// function libhegel exports as `hegel_label_from_name` — so it can be
/// computed at compile time:
///
/// ```
/// use hegel::generators as gs;
///
/// const PAIRS_LABEL: u64 = gs::label_from_name("mycrate.pairs");
/// ```
///
/// A name only has to be stable and unique to its generator; prefixing it
/// with the crate's name keeps it clear of Hegel's own `hegel.<kind>` names.
pub const fn label_from_name(name: &str) -> u64 {
    fnv1a(FNV_OFFSET_BASIS, name.as_bytes())
}

/// The span label for a generator built from other generators.
///
/// Combines the given labels, in order, into one — the same function
/// libhegel exports as `hegel_label_combine`. Pass the generator's own label
/// (from [`label_from_name`]) first and its components' labels after it, so
/// that a pair of integers and a pair of strings get different labels while
/// every pair of integers gets the same one:
///
/// ```
/// use hegel::generators::{self as gs, Generator};
///
/// struct Pairs<G> {
///     inner: G,
/// }
///
/// impl<T, G: Generator<T>> Generator<(T, T)> for Pairs<G> {
///     fn label(&self) -> u64 {
///         gs::combine_labels(&[gs::label_from_name("mycrate.pairs"), self.inner.label()])
///     }
///
///     fn do_draw(&self, tc: &hegel::TestCase) -> (T, T) {
///         (self.inner.do_draw(tc), self.inner.do_draw(tc))
///     }
/// }
/// ```
///
/// Combining is order-sensitive, and combining a single label does not
/// return it unchanged.
pub const fn combine_labels(labels: &[u64]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    let mut i = 0;
    while i < labels.len() {
        hash = fnv1a(hash, &labels[i].to_le_bytes());
        i += 1;
    }
    hash
}

#[cfg(test)]
#[path = "../../tests/embedded/generators/labels_tests.rs"]
mod tests;
