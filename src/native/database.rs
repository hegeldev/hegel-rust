// Persistence layer for the native backend.
//
// A multi-value key/value store where each key maps to a *set* of values.
// The `TestCaseDatabase` trait captures the shared surface
// (`save` / `fetch` / `delete` / `move_value`); `DirectoryTestCaseDatabase`
// is the directory-backed implementation and `InMemoryNativeDatabase` is
// a non-persistent sibling.
//
// Minimal-native: the change-listener / watcher infrastructure, the
// `ReadOnly` / `Multiplexed` / `BackgroundWrite` wrapper databases, and
// the cross-process tempfile-rename dance live in the full native
// branch but are not part of this minimal version.
//
// # On-disk format
//
// Storage layout (directory backend):
//
//   db_root/<key_hash(key)>/<fnv_hex(value)>
//
// where `key_hash(k) = fnv_hex(b"native:" ++ k)` and the file contents
// are the raw value bytes.  `serialize_choices` and `deserialize_choices`
// are the canonical binary encoding used for ChoiceValue sequences (the
// value bytes); they are kept here so that the replay path in
// `test_runner.rs` can round-trip them.
//
// The `native:` key prefix ensures that even if a user accidentally
// points `database` at a directory containing another store, our hashes
// are disjoint and the two stores can't overwrite each other's entries.
// It also leaves room for a future `core:`-prefixed store (the eventual
// full hegel-core backend) to live at the same `db_root`.

use std::path::PathBuf;

use crate::native::bignum::BigInt;
use crate::native::core::ChoiceValue;

/// Multi-value key/value store backing the native engine's replay phase.
///
/// Each key maps to an unordered *set* of values. Implementations must
/// tolerate concurrent or corrupt state and surface failures as silent
/// no-ops rather than errors — a non-writable database must never abort
/// an otherwise-successful test run.
pub trait TestCaseDatabase: Send + Sync {
    /// Return every value stored under `key`, in arbitrary order. Returns
    /// an empty `Vec` if the key is absent.
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>>;

    /// Add `value` to the set stored under `key`. Idempotent: saving a
    /// value that is already present is a no-op.
    fn save(&self, key: &[u8], value: &[u8]);

    /// Remove `value` from the set stored under `key`. A no-op when
    /// `value` is absent.
    fn delete(&self, key: &[u8], value: &[u8]);

    /// Move `value` from `src` to `dst`. `value` is inserted at `dst`
    /// regardless of whether it was present at `src`.
    ///
    /// Named `move_value` rather than `move` because `move` is a Rust
    /// keyword.
    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]);
}

/// Name of the bookkeeping key under which every save() records its
/// own key bytes.
pub const METAKEYS_NAME: &[u8] = b".hegel-keys";

/// Prefix prepended to every key before it's hashed onto disk. Keeps
/// `DirectoryTestCaseDatabase`'s on-disk hashes disjoint from any other
/// store that happens to share `db_root` (e.g. a future hegel `core:`
/// store): the formats aren't cross-compatible, so we never want their
/// paths to coincide.
const KEY_PREFIX: &[u8] = b"native:";

fn key_hash(key: &[u8]) -> String {
    let mut buf = Vec::with_capacity(KEY_PREFIX.len() + key.len());
    buf.extend_from_slice(KEY_PREFIX);
    buf.extend_from_slice(key);
    fnv_hex(&buf)
}

pub struct DirectoryTestCaseDatabase {
    db_root: PathBuf,
    metakeys_hash: String,
}

impl DirectoryTestCaseDatabase {
    pub fn new(db_root: &str) -> Self {
        DirectoryTestCaseDatabase {
            db_root: PathBuf::from(db_root),
            metakeys_hash: key_hash(METAKEYS_NAME),
        }
    }

    pub fn key_path(&self, key: &[u8]) -> PathBuf {
        self.db_root.join(key_hash(key))
    }

    fn value_path(&self, key: &[u8], value: &[u8]) -> PathBuf {
        self.key_path(key).join(fnv_hex(value))
    }
}

impl TestCaseDatabase for DirectoryTestCaseDatabase {
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>> {
        let dir = self.key_path(key);
        let entries = match std::fs::read_dir(&dir) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                out.push(bytes);
            }
        }
        out
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        // The "metakeys" entry is a bookkeeping key whose values are the
        // raw bytes of every other key ever saved. Avoid infinite
        // recursion when we're already saving under it.
        if key_hash(key) != self.metakeys_hash {
            self.save(METAKEYS_NAME, key);
        }
        let dir = self.key_path(key);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let path = self.value_path(key, value);
        if path.exists() {
            return;
        }
        let _ = std::fs::write(&path, value);
    }

    fn delete(&self, key: &[u8], value: &[u8]) {
        if std::fs::remove_file(self.value_path(key, value)).is_err() {
            return;
        }
        // `remove_dir` only succeeds if the directory is empty; that's
        // exactly the "value was the last entry" case.
        if std::fs::remove_dir(self.key_path(key)).is_ok() && key_hash(key) != self.metakeys_hash {
            self.delete(METAKEYS_NAME, key);
        }
    }

    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]) {
        if src == dst {
            self.save(src, value);
            return;
        }
        if !self.key_path(dst).exists() {
            self.save(METAKEYS_NAME, dst);
        }
        let dst_dir = self.key_path(dst);
        if std::fs::create_dir_all(&dst_dir).is_err() {
            self.delete(src, value);
            self.save(dst, value);
            return;
        }
        let src_path = self.value_path(src, value);
        let dst_path = self.value_path(dst, value);
        if std::fs::rename(&src_path, &dst_path).is_err() {
            self.delete(src, value);
            self.save(dst, value);
            return;
        }
        let _ = std::fs::remove_dir(self.key_path(src));
    }
}

/// FNV-1a 64-bit hash of a byte slice, formatted as a 16-character hex
/// string.
///
/// We hash keys and values onto the filesystem to give every entry a
/// fixed-width, path-safe name regardless of the raw bytes: database
/// keys are arbitrary user-chosen strings (e.g. test names with
/// `::` separators) and values are CBOR-encoded choice sequences full
/// of arbitrary bytes.  FNV-1a is fine here because we only need
/// collision-avoidance, not cryptographic security.
pub(super) fn fnv_hex(s: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in s {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Binary encoding of a `ChoiceValue` slice.
///
/// Format:
/// - 4-byte little-endian u32: number of choices
/// - For each choice:
///   - 1-byte type tag: 0=Integer, 1=Boolean, 2=Float, 3=Bytes, 4=String
///   - Value bytes:
///     - Integer: a 1-byte width sub-tag (always 10=BigInt for new writes;
///       sub-tags 0–9 are still accepted on read for backward compat)
///       followed by a 4-byte little-endian length and that many
///       two's-complement little-endian bytes
///     - Boolean: 1 byte (0 or 1)
///     - Float: 8 bytes (the f64 bit pattern, little-endian, so `-0.0` and
///       NaN payloads round-trip unchanged)
///     - Bytes: 4 bytes (u32 little-endian length) followed by the raw bytes
///     - String: 4 bytes (u32 little-endian codepoint count) followed by
///       4 bytes per codepoint (u32 little-endian)
pub fn serialize_choices(choices: &[ChoiceValue]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + choices.len() * 17);
    let count = choices.len() as u32;
    buf.extend_from_slice(&count.to_le_bytes());
    for choice in choices {
        match choice {
            ChoiceValue::Integer(v) => {
                buf.push(0);
                serialize_any_integer(&mut buf, v);
            }
            ChoiceValue::Boolean(v) => {
                buf.push(1);
                buf.push(*v as u8);
            }
            ChoiceValue::Float(v) => {
                buf.push(2);
                buf.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            ChoiceValue::Bytes(v) => {
                buf.push(3);
                let len = v.len() as u32;
                buf.extend_from_slice(&len.to_le_bytes());
                buf.extend_from_slice(v);
            }
            ChoiceValue::String(v) => {
                buf.push(4);
                let len = v.len() as u32;
                buf.extend_from_slice(&len.to_le_bytes());
                for &cp in v {
                    buf.extend_from_slice(&cp.to_le_bytes());
                }
            }
        }
    }
    buf
}

/// Encode a [`BigInt`] as sub-tag 10 followed by a length-prefixed
/// two's-complement little-endian byte sequence (see [`serialize_choices`]).
fn serialize_any_integer(buf: &mut Vec<u8>, v: &BigInt) {
    buf.push(10);
    let mag = v.to_signed_bytes_le();
    buf.extend_from_slice(&(mag.len() as u32).to_le_bytes());
    buf.extend_from_slice(&mag);
}

/// Inverse of [`serialize_any_integer`]. Returns the decoded `BigInt` and the
/// new read position, or `None` on truncation / an unknown width sub-tag.
///
/// Sub-tags 0–9 (legacy per-width formats) are still accepted for backward
/// compatibility; they are converted to `BigInt` on read.
fn deserialize_any_integer(bytes: &[u8], pos: usize) -> Option<(BigInt, usize)> {
    let sub = *bytes.get(pos)?;
    let mut pos = pos + 1;
    macro_rules! native {
        ($t:ty) => {{
            const N: usize = std::mem::size_of::<$t>();
            let raw: [u8; N] = bytes.get(pos..pos + N)?.try_into().ok()?;
            pos += N;
            BigInt::from(<$t>::from_le_bytes(raw))
        }};
    }
    let value = match sub {
        0 => native!(i8),
        1 => native!(i16),
        2 => native!(i32),
        3 => native!(i64),
        4 => native!(i128),
        5 => native!(u8),
        6 => native!(u16),
        7 => native!(u32),
        8 => native!(u64),
        9 => native!(u128),
        10 => {
            let len_raw: [u8; 4] = bytes.get(pos..pos + 4)?.try_into().ok()?;
            pos += 4;
            let len = u32::from_le_bytes(len_raw) as usize;
            let mag = bytes.get(pos..pos + len)?;
            pos += len;
            BigInt::from_signed_bytes_le(mag)
        }
        _ => return None,
    };
    Some((value, pos))
}

/// Decode a byte slice produced by [`serialize_choices`].
///
/// Returns `None` if the data is truncated, malformed, or contains an
/// unknown type tag (defensive against filesystem corruption).
pub fn deserialize_choices(bytes: &[u8]) -> Option<Vec<ChoiceValue>> {
    if bytes.len() < 4 {
        return None;
    }
    let count = u32::from_le_bytes(bytes[..4].try_into().ok()?) as usize;
    // A corrupted entry can claim a count far larger than the input
    // buffer can possibly back.  Cap pre-allocation at the buffer
    // length so a bogus `count = u32::MAX` doesn't OOM the process.
    let mut choices = Vec::with_capacity(count.min(bytes.len()));
    let mut pos = 4;
    for _ in 0..count {
        if pos >= bytes.len() {
            return None;
        }
        match bytes[pos] {
            0 => {
                pos += 1;
                let (value, new_pos) = deserialize_any_integer(bytes, pos)?;
                pos = new_pos;
                choices.push(ChoiceValue::Integer(value));
            }
            1 => {
                pos += 1;
                if pos >= bytes.len() {
                    return None;
                }
                choices.push(ChoiceValue::Boolean(bytes[pos] != 0));
                pos += 1;
            }
            2 => {
                pos += 1;
                if pos + 8 > bytes.len() {
                    return None;
                }
                let bits = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
                choices.push(ChoiceValue::Float(f64::from_bits(bits)));
                pos += 8;
            }
            3 => {
                pos += 1;
                if pos + 4 > bytes.len() {
                    return None;
                }
                let len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
                pos += 4;
                if pos + len > bytes.len() {
                    return None;
                }
                choices.push(ChoiceValue::Bytes(bytes[pos..pos + len].to_vec()));
                pos += len;
            }
            4 => {
                pos += 1;
                if pos + 4 > bytes.len() {
                    return None;
                }
                let len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
                pos += 4;
                if pos + len.checked_mul(4)? > bytes.len() {
                    return None;
                }
                let mut cps = Vec::with_capacity(len);
                for _ in 0..len {
                    let cp = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
                    cps.push(cp);
                    pos += 4;
                }
                choices.push(ChoiceValue::String(cps));
            }
            _ => return None,
        }
    }
    Some(choices)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/database_tests.rs"]
mod tests;
