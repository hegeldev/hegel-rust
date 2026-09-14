//! Span labels: opaque 64-bit values identifying which generator a span
//! came from.
//!
//! A label carries no meaning beyond identity: the engine treats two spans
//! with the same label as coming from the same generator (and so as
//! candidates for swapping, duplicating and reordering), and that is all it
//! does with them. Labels are derived from names by [`label_from_name`] and
//! from other labels by [`combine_labels`]; the engine's own spans use names
//! of the form `hegel.<kind>`. Both functions are exported over the C ABI as
//! `hegel_label_from_name` / `hegel_label_combine` so every frontend derives
//! labels the same way.

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

/// The label for a name given as bytes: their 64-bit FNV-1a hash.
pub const fn label_from_bytes(name: &[u8]) -> u64 {
    fnv1a(FNV_OFFSET_BASIS, name)
}

/// The label for a name: the 64-bit FNV-1a hash of its UTF-8 bytes.
pub const fn label_from_name(name: &str) -> u64 {
    label_from_bytes(name.as_bytes())
}

/// The label for a generator built from others: the 64-bit FNV-1a hash of
/// the given labels' little-endian bytes, in order. Order matters, and
/// combining a single label does not return it unchanged.
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
#[path = "../../tests/embedded/native/labels_tests.rs"]
mod tests;
