// Persistence layer for the native backend.
//
// Mirrors Hypothesis's `ExampleDatabase` hierarchy
// (resources/hypothesis/hypothesis-python/src/hypothesis/database.py): a
// multi-value key/value store where each key maps to a *set* of values.
// The `ExampleDatabase` trait captures the shared surface
// (`save` / `fetch` / `delete` / `move_value`); `NativeDatabase` is the
// directory-backed implementation (mirroring
// `DirectoryBasedExampleDatabase`) and `InMemoryNativeDatabase` is a
// non-persistent sibling (mirroring `InMemoryExampleDatabase`).
//
// pbtkit's `DirectoryDB` (`resources/pbtkit/src/pbtkit/database.py`)
// deliberately simplified this to a single-value store. The richer
// Hypothesis model is needed so that the replay phase can retain more
// than one candidate counterexample per key (see
// `reuse_existing_examples` in `conjecture/engine.py`), so the native
// engine follows Hypothesis here.
//
// Storage layout (directory backend):
//   db_root/<fnv_hex(key)>/<fnv_hex(value)>
//
// where the file contents are the raw value bytes. `serialize_choices`
// and `deserialize_choices` are the canonical binary encoding used for
// ChoiceValue sequences (the value bytes); they are kept here so that
// the replay path in `runner.rs` can round-trip them.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use crate::native::core::ChoiceValue;

/// Multi-value key/value store backing the native engine's replay phase.
///
/// Mirrors Hypothesis's `ExampleDatabase` base class
/// (`hypothesis/database.py`): each key maps to an unordered *set* of
/// values. Implementations must tolerate concurrent or corrupt state and
/// surface failures as silent no-ops rather than errors — a non-writable
/// database must never abort an otherwise-successful test run.
pub trait ExampleDatabase: Send + Sync {
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
    /// keyword. Hypothesis: `ExampleDatabase.move`. The default
    /// implementation is `delete` + `save`; backends may override for
    /// atomicity (e.g. `NativeDatabase` uses `rename`). No internal
    /// caller yet — kept to match the Hypothesis spec for wrappers such
    /// as `MultiplexedDatabase` (not yet ported) that rely on it.
    #[allow(dead_code)]
    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]) {
        if src == dst {
            self.save(src, value);
            return;
        }
        self.delete(src, value);
        self.save(dst, value);
    }
}

/// Let `Arc<T>` stand in for an `ExampleDatabase` wherever the trait is
/// required, so callers can keep their own handle on an inner database
/// (and read it back) while also passing it into a wrapper such as
/// `ReadOnlyNativeDatabase` or `MultiplexedNativeDatabase`.
impl<T: ExampleDatabase + ?Sized> ExampleDatabase for Arc<T> {
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>> {
        (**self).fetch(key)
    }
    fn save(&self, key: &[u8], value: &[u8]) {
        (**self).save(key, value);
    }
    fn delete(&self, key: &[u8], value: &[u8]) {
        (**self).delete(key, value);
    }
    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]) {
        (**self).move_value(src, dst, value);
    }
}

pub struct NativeDatabase {
    db_root: PathBuf,
}

impl NativeDatabase {
    pub fn new(db_root: &str) -> Self {
        NativeDatabase {
            db_root: PathBuf::from(db_root),
        }
    }

    fn key_path(&self, key: &[u8]) -> PathBuf {
        self.db_root.join(fnv_hex(key))
    }

    fn value_path(&self, key: &[u8], value: &[u8]) -> PathBuf {
        self.key_path(key).join(fnv_hex(value))
    }
}

impl ExampleDatabase for NativeDatabase {
    /// Hypothesis: `DirectoryBasedExampleDatabase.fetch`. Returns an
    /// empty `Vec` if the key is absent or the directory is unreadable.
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

    /// Hypothesis: `DirectoryBasedExampleDatabase.save`. I/O errors are
    /// silently ignored.
    fn save(&self, key: &[u8], value: &[u8]) {
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

    /// Hypothesis: `DirectoryBasedExampleDatabase.delete`. If `value` was
    /// the last entry under `key`, the (now-empty) key directory is also
    /// removed.
    fn delete(&self, key: &[u8], value: &[u8]) {
        if std::fs::remove_file(self.value_path(key, value)).is_err() {
            return;
        }
        // `remove_dir` only succeeds if the directory is empty; that's
        // exactly the "value was the last entry" case.
        let _ = std::fs::remove_dir(self.key_path(key));
    }

    /// Hypothesis: `DirectoryBasedExampleDatabase.move`. Overrides the
    /// default `delete` + `save` with a single `rename` when possible so
    /// that the move is atomic on the same filesystem.
    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]) {
        if src == dst {
            self.save(src, value);
            return;
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
        // Cleanup: if `src`'s key directory is now empty, remove it.
        let _ = std::fs::remove_dir(self.key_path(src));
    }
}

/// Non-persistent sibling of [`NativeDatabase`]. Backing store is a
/// `HashMap<Vec<u8>, HashSet<Vec<u8>>>` behind a `Mutex`.
///
/// Hypothesis: `InMemoryExampleDatabase`. Useful when the replay
/// machinery needs a database that doesn't survive the process, e.g.
/// in tests that exercise the `ExampleDatabase` contract against
/// multiple backends. Not currently wired into the public `Settings`
/// surface — exposed via the trait for test use.
#[allow(dead_code)]
pub struct InMemoryNativeDatabase {
    data: Mutex<HashMap<Vec<u8>, HashSet<Vec<u8>>>>,
}

#[allow(dead_code)]
impl InMemoryNativeDatabase {
    pub fn new() -> Self {
        InMemoryNativeDatabase {
            data: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryNativeDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl ExampleDatabase for InMemoryNativeDatabase {
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>> {
        let data = self.data.lock().unwrap();
        data.get(key)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        let mut data = self.data.lock().unwrap();
        data.entry(key.to_vec()).or_default().insert(value.to_vec());
    }

    fn delete(&self, key: &[u8], value: &[u8]) {
        let mut data = self.data.lock().unwrap();
        if let Some(values) = data.get_mut(key) {
            values.remove(value);
        }
    }
}

/// Read-only view of another database: `fetch` forwards to the inner
/// database; `save` / `delete` / `move_value` are silent no-ops.
///
/// Hypothesis: `ReadOnlyDatabase`. Useful for exposing a shared database
/// (e.g. CI-populated) to developer machines without letting local runs
/// propagate changes back.
#[allow(dead_code)]
pub struct ReadOnlyNativeDatabase<D: ExampleDatabase> {
    inner: D,
}

#[allow(dead_code)]
impl<D: ExampleDatabase> ReadOnlyNativeDatabase<D> {
    pub fn new(inner: D) -> Self {
        Self { inner }
    }
}

impl<D: ExampleDatabase> ExampleDatabase for ReadOnlyNativeDatabase<D> {
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>> {
        self.inner.fetch(key)
    }
    fn save(&self, _key: &[u8], _value: &[u8]) {}
    fn delete(&self, _key: &[u8], _value: &[u8]) {}
    fn move_value(&self, _src: &[u8], _dst: &[u8], _value: &[u8]) {}
}

/// Fan-out wrapper that multiplexes writes across several databases and
/// unions their reads.
///
/// Hypothesis: `MultiplexedDatabase`. `save` / `delete` / `move_value`
/// run against every inner database; `fetch` returns the union of each
/// inner database's values, de-duplicated so that a value present in
/// multiple backends is yielded once. Inner databases are held behind
/// `Arc` so callers can retain their own handles and observe the writes
/// landing (the `test_multiplexed_dbs_read_and_write_all` test checks
/// each backing database individually).
#[allow(dead_code)]
pub struct MultiplexedNativeDatabase {
    inner: Vec<Arc<dyn ExampleDatabase>>,
}

#[allow(dead_code)]
impl MultiplexedNativeDatabase {
    pub fn new(inner: Vec<Arc<dyn ExampleDatabase>>) -> Self {
        Self { inner }
    }
}

impl ExampleDatabase for MultiplexedNativeDatabase {
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>> {
        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        let mut out = Vec::new();
        for db in &self.inner {
            for v in db.fetch(key) {
                if seen.insert(v.clone()) {
                    out.push(v);
                }
            }
        }
        out
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        for db in &self.inner {
            db.save(key, value);
        }
    }

    fn delete(&self, key: &[u8], value: &[u8]) {
        for db in &self.inner {
            db.delete(key, value);
        }
    }

    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]) {
        for db in &self.inner {
            db.move_value(src, dst, value);
        }
    }
}

enum BackgroundTask {
    Save(Vec<u8>, Vec<u8>),
    Delete(Vec<u8>, Vec<u8>),
    Move(Vec<u8>, Vec<u8>, Vec<u8>),
}

struct BackgroundQueue {
    state: Mutex<BackgroundQueueState>,
    not_empty: Condvar,
    all_done: Condvar,
}

struct BackgroundQueueState {
    tasks: VecDeque<BackgroundTask>,
    // `pending` counts queued-but-not-yet-processed tasks *plus* the
    // task currently in flight, so `fetch` can block until every
    // enqueued write has actually run against the inner database.
    pending: usize,
    shutdown: bool,
}

/// Wrapper that defers writes to a background worker thread so that
/// `save` / `delete` / `move_value` return quickly. `fetch` blocks
/// until the queue drains so reads see every previously-enqueued write.
///
/// Hypothesis: `BackgroundWriteDatabase`. Python uses `queue.Queue` +
/// `threading.Thread` + `weakref.finalize` to flush on GC; Rust uses
/// a `Mutex<VecDeque>` + `Condvar` and flushes on `Drop`.
#[allow(dead_code)]
pub struct BackgroundWriteNativeDatabase {
    inner: Arc<dyn ExampleDatabase>,
    queue: Arc<BackgroundQueue>,
    handle: Option<JoinHandle<()>>,
}

#[allow(dead_code)]
impl BackgroundWriteNativeDatabase {
    pub fn new<D: ExampleDatabase + 'static>(db: D) -> Self {
        let inner: Arc<dyn ExampleDatabase> = Arc::new(db);
        let queue = Arc::new(BackgroundQueue {
            state: Mutex::new(BackgroundQueueState {
                tasks: VecDeque::new(),
                pending: 0,
                shutdown: false,
            }),
            not_empty: Condvar::new(),
            all_done: Condvar::new(),
        });
        let worker_inner = Arc::clone(&inner);
        let worker_queue = Arc::clone(&queue);
        let handle = thread::spawn(move || background_worker_loop(worker_inner, worker_queue));
        Self {
            inner,
            queue,
            handle: Some(handle),
        }
    }

    fn enqueue(&self, task: BackgroundTask) {
        let mut state = self.queue.state.lock().unwrap();
        state.tasks.push_back(task);
        state.pending += 1;
        self.queue.not_empty.notify_one();
    }

    fn wait_all_done(&self) {
        let mut state = self.queue.state.lock().unwrap();
        while state.pending > 0 {
            state = self.queue.all_done.wait(state).unwrap();
        }
    }
}

fn background_worker_loop(inner: Arc<dyn ExampleDatabase>, queue: Arc<BackgroundQueue>) {
    loop {
        let task = {
            let mut state = queue.state.lock().unwrap();
            while state.tasks.is_empty() && !state.shutdown {
                state = queue.not_empty.wait(state).unwrap();
            }
            match state.tasks.pop_front() {
                Some(t) => t,
                None => return, // shutdown signalled and queue drained
            }
        };
        match task {
            BackgroundTask::Save(k, v) => inner.save(&k, &v),
            BackgroundTask::Delete(k, v) => inner.delete(&k, &v),
            BackgroundTask::Move(src, dst, v) => inner.move_value(&src, &dst, &v),
        }
        let mut state = queue.state.lock().unwrap();
        state.pending -= 1;
        if state.pending == 0 {
            queue.all_done.notify_all();
        }
    }
}

impl Drop for BackgroundWriteNativeDatabase {
    fn drop(&mut self) {
        {
            let mut state = self.queue.state.lock().unwrap();
            state.shutdown = true;
            self.queue.not_empty.notify_all();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl ExampleDatabase for BackgroundWriteNativeDatabase {
    fn fetch(&self, key: &[u8]) -> Vec<Vec<u8>> {
        self.wait_all_done();
        self.inner.fetch(key)
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        self.enqueue(BackgroundTask::Save(key.to_vec(), value.to_vec()));
    }

    fn delete(&self, key: &[u8], value: &[u8]) {
        self.enqueue(BackgroundTask::Delete(key.to_vec(), value.to_vec()));
    }

    fn move_value(&self, src: &[u8], dst: &[u8], value: &[u8]) {
        self.enqueue(BackgroundTask::Move(
            src.to_vec(),
            dst.to_vec(),
            value.to_vec(),
        ));
    }
}

/// FNV-1a 64-bit hash of a byte slice, formatted as a 16-character hex string.
///
/// Used to map database keys and values to directory / file names so that
/// arbitrary binary inputs are safe to use as filesystem path components.
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
///     - Integer: 16 bytes (i128 little-endian)
///     - Boolean: 1 byte (0 or 1)
///     - Float: 8 bytes (u64 bit representation, little-endian)
///     - Bytes: 4-byte le u32 length, then that many raw bytes
///     - String: 4-byte le u32 codepoint count, then that many 4-byte
///       little-endian u32 codepoints (raw Unicode codepoints, including
///       surrogates — the engine's internal codepoint model preserves them;
///       the no-surrogate filter lives at the user-facing boundary).
pub(super) fn serialize_choices(choices: &[ChoiceValue]) -> Vec<u8> {
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

/// Decode a byte slice produced by [`serialize_choices`].
///
/// Returns `None` if the data is truncated, malformed, or contains an
/// unknown type tag (defensive against filesystem corruption).
pub(super) fn deserialize_choices(bytes: &[u8]) -> Option<Vec<ChoiceValue>> {
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
                let count = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
                pos += 4;
                let byte_len = count.checked_mul(4)?;
                if pos + byte_len > bytes.len() {
                    return None;
                }
                let mut cps: Vec<u32> = Vec::with_capacity(count);
                for _ in 0..count {
                    let cp = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
                    // Guard against out-of-range codepoints from a corrupt
                    // database entry — real values lie in `0..=0x10FFFF`.
                    if cp > 0x10FFFF {
                        return None;
                    }
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
