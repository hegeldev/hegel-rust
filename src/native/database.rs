// Persistence layer for the native backend.
//
// Mirrors Hypothesis's `TestCaseDatabase` hierarchy
// (resources/hypothesis/hypothesis-python/src/hypothesis/database.py): a
// multi-value key/value store where each key maps to a *set* of values.
// The `TestCaseDatabase` trait captures the shared surface
// (`save` / `fetch` / `delete` / `move_value`); `DirectoryTestCaseDatabase` is the
// directory-backed implementation (mirroring
// `DirectoryBasedExampleDatabase`) and `InMemoryNativeDatabase` is a
// non-persistent sibling (mirroring `InMemoryExampleDatabase`).
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
// The on-disk format is deliberately not cross-compatible with
// Hypothesis's `DirectoryBasedExampleDatabase`.  The `native:` key
// prefix means even if a user accidentally points `database` at
// `.hypothesis/examples`, our hashes are disjoint from Hypothesis's
// and the two stores can't overwrite each other's entries.  It also
// leaves room for a future `core:`-prefixed store (the eventual full
// hegel-core backend) to live at the same `db_root`.

use std::path::PathBuf;

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
/// own key bytes. Mirrors Hypothesis's
/// `DirectoryBasedExampleDatabase._metakeys_name` (`.hypothesis-keys`).
pub const METAKEYS_NAME: &[u8] = b".hegel-keys";

/// Prefix prepended to every key before it's hashed onto disk.  Keeps
/// `DirectoryTestCaseDatabase`'s on-disk hashes disjoint from a Hypothesis store
/// (or a future hegel `core:` store) that happens to share `db_root`:
/// the formats aren't cross-compatible, so we never want their paths
/// to coincide.
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
        // Hypothesis keeps a "metakeys" entry — a bookkeeping key whose
        // values are the raw bytes of every other key ever saved. Avoid
        // infinite recursion when we're already saving under it.
        if key_hash(key) != self.metakeys_hash {
            self.save(METAKEYS_NAME, key);
        }
        let dir = self.key_path(key);
        if std::fs::create_dir_all(&dir).is_err() {
            return; // nocov — filesystem permission denial, not reachable in tests
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
        // Filesystem permission denial; the dst_dir create_dir_all
        // call always succeeds in the test harness.
        // nocov start
        if std::fs::create_dir_all(&dst_dir).is_err() {
            self.delete(src, value);
            self.save(dst, value);
            return;
        }
        // nocov end
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
///   - 1-byte type tag: 0=Integer, 1=Boolean
///   - Value bytes:
///     - Integer: 16 bytes (i128 little-endian)
///     - Boolean: 1 byte (0 or 1)
///
/// Minimal-native only supports integer and boolean choice nodes;
/// attempting to serialize any other variant panics with `todo!()`.
pub fn serialize_choices(choices: &[ChoiceValue]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + choices.len() * 17);
    let count = choices.len() as u32;
    buf.extend_from_slice(&count.to_le_bytes());
    for choice in choices {
        match choice {
            ChoiceValue::Integer(v) => {
                buf.push(0);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            ChoiceValue::Boolean(v) => {
                buf.push(1);
                buf.push(*v as u8);
            }
        }
    }
    buf
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
                if pos + 16 > bytes.len() {
                    return None;
                }
                let v = i128::from_le_bytes(bytes[pos..pos + 16].try_into().ok()?);
                choices.push(ChoiceValue::Integer(v));
                pos += 16;
            }
            1 => {
                pos += 1;
                if pos >= bytes.len() {
                    return None;
                }
                choices.push(ChoiceValue::Boolean(bytes[pos] != 0));
                pos += 1;
            }
            _ => return None,
        }
    }
    Some(choices)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/database_tests.rs"]
mod tests;
