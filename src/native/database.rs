// Persistence layer for the native backend.
//
// Stores shrunk counterexamples keyed by `database_key` so that subsequent
// runs can replay known failures immediately without repeating the full
// generation + shrink cycle.
//
// Storage layout:
//   db_root/<fnv_hex(key)>/best
//
// where `best` contains a binary-encoded sequence of ChoiceValue records.

use std::path::PathBuf;

use crate::native::core::ChoiceValue;

pub struct NativeDatabase {
    db_root: PathBuf,
}

impl NativeDatabase {
    pub fn new(db_root: &str) -> Self {
        NativeDatabase {
            db_root: PathBuf::from(db_root),
        }
    }

    fn key_dir(&self, key: &str) -> PathBuf {
        self.db_root.join(fnv_hex(key))
    }

    /// Load the stored choice sequence for `key`, or `None` if nothing is stored.
    pub fn load(&self, key: &str) -> Option<Vec<ChoiceValue>> {
        let path = self.key_dir(key).join("best");
        let bytes = std::fs::read(&path).ok()?;
        deserialize_choices(&bytes)
    }

    /// Persist `choices` as the best known counterexample for `key`.
    ///
    /// Silently ignores I/O errors so that a non-writable database does not
    /// abort an otherwise-successful test run.
    pub fn save(&self, key: &str, choices: &[ChoiceValue]) {
        let dir = self.key_dir(key);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let path = dir.join("best");
        let bytes = serialize_choices(choices);
        let _ = std::fs::write(&path, bytes);
    }
}

/// FNV-1a 64-bit hash of a string, formatted as a 16-character hex string.
///
/// Used to map database keys to directory names so that arbitrary key strings
/// are safe to use as filesystem path components.
fn fnv_hex(s: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in s.bytes() {
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
///     - Integer: 16 bytes (i128 little-endian)
///     - Boolean: 1 byte (0 or 1)
///     - Float: 8 bytes (u64 bit representation, little-endian)
///     - Bytes: 4-byte le u32 length, then that many raw bytes
///     - String: 4-byte le u32 byte-length, then that many UTF-8 bytes
fn serialize_choices(choices: &[ChoiceValue]) -> Vec<u8> {
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
                let bytes = v.as_bytes();
                let len = bytes.len() as u32;
                buf.extend_from_slice(&len.to_le_bytes());
                buf.extend_from_slice(bytes);
            }
        }
    }
    buf
}

/// Decode a byte slice produced by [`serialize_choices`].
///
/// Returns `None` if the data is truncated, malformed, or contains an
/// unknown type tag (defensive against filesystem corruption).
fn deserialize_choices(bytes: &[u8]) -> Option<Vec<ChoiceValue>> {
    if bytes.len() < 4 {
        return None;
    }
    let count = u32::from_le_bytes(bytes[..4].try_into().ok()?) as usize;
    let mut choices = Vec::with_capacity(count);
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
                if pos + len > bytes.len() {
                    return None;
                }
                let s = std::str::from_utf8(&bytes[pos..pos + len])
                    .ok()?
                    .to_string();
                choices.push(ChoiceValue::String(s));
                pos += len;
            }
            _ => return None,
        }
    }
    Some(choices)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/database_tests.rs"]
mod tests;
