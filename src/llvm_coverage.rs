//! The fuzzer's coverage sink: this process's LLVM instrumentation
//! counters, written to a file after every test case for a fuzzer that
//! drives the program through the engine's `HEGEL_FUZZ_*` variables.
//!
//! When the program was built with `-C instrument-coverage` and
//! `HEGEL_FUZZ_COVERAGE` names a file, the counters are noted before each
//! test case and, once it has run, what the case added to each is written
//! to the file as one byte per counter: the count bucketed as AFL buckets
//! it, with 0, 1, 2 and 3 kept, 4–7, 8–15, 16–31 and 32–127 becoming 4 to
//! 7, and anything larger 8. A fuzzer then reads a case's coverage as a
//! small flat map, instead of parsing the raw profile the process writes
//! at exit; the counters themselves are never touched, so that profile is
//! what it would have been. A program built without instrumentation has no
//! counters, and setting the variable for it is an error.

use std::path::PathBuf;

/// The environment variable naming the file the map goes to.
pub(crate) const VAR: &str = "HEGEL_FUZZ_COVERAGE";

/// The AFL bucket of a counter's value.
#[cfg(any(hegel_coverage, test))]
fn bucket(count: u64) -> u8 {
    match count {
        0..=3 => count as u8,
        4..=7 => 4,
        8..=15 => 5,
        16..=31 => 6,
        32..=127 => 7,
        _ => 8,
    }
}

/// The counters of an instrumented program, as the profiling runtime
/// linked into it exposes them.
#[cfg(hegel_coverage)]
mod counters {
    unsafe extern "C" {
        fn __llvm_profile_begin_counters() -> *const u8;
        fn __llvm_profile_end_counters() -> *const u8;
        fn __llvm_profile_counter_entry_size() -> core::ffi::c_uint;
    }

    /// Every counter's current value, in order.
    pub(super) fn values(into: &mut Vec<u64>) {
        let (begin, end, size) = unsafe {
            (
                __llvm_profile_begin_counters(),
                __llvm_profile_end_counters(),
                __llvm_profile_counter_entry_size() as usize,
            )
        };
        let bytes = unsafe { core::slice::from_raw_parts(begin, end.offset_from(begin) as usize) };
        into.clear();
        for counter in bytes.chunks_exact(size.max(1)) {
            let mut value = [0u8; 8];
            let width = counter.len().min(8);
            value[..width].copy_from_slice(&counter[..width]);
            into.push(u64::from_le_bytes(value));
        }
    }
}

/// The map's destination and the buffer it is built in.
#[cfg(hegel_coverage)]
pub(crate) struct Sink {
    path: PathBuf,
    before: Vec<u64>,
    now: Vec<u64>,
    map: Vec<u8>,
}

#[cfg(hegel_coverage)]
impl Sink {
    /// The sink `HEGEL_FUZZ_COVERAGE` asks for, if it is set.
    pub(crate) fn from_env() -> Option<Sink> {
        Some(Sink::new(std::env::var_os(VAR)?.into()))
    }

    pub(crate) fn new(path: PathBuf) -> Sink {
        Sink {
            path,
            before: Vec::new(),
            now: Vec::new(),
            map: Vec::new(),
        }
    }

    /// Note the counters as they stand: the next map covers only what
    /// runs from here.
    pub(crate) fn begin(&mut self) {
        counters::values(&mut self.before);
    }

    /// Write the map of everything counted since [`Self::begin`].
    pub(crate) fn write(&mut self) {
        counters::values(&mut self.now);
        self.map.clear();
        self.map.extend(
            self.now
                .iter()
                .zip(&self.before)
                .map(|(now, before)| bucket(now.wrapping_sub(*before))),
        );
        if let Err(e) = std::fs::write(&self.path, &self.map) {
            panic!("{VAR}: cannot write {}: {e}", self.path.display());
        }
    }
}

/// Note the counters before a test case, if there is a sink. Kept out of
/// the generic run driver so that its lines belong to this crate's own
/// code, which the coverage run can see, rather than to the driver's
/// instantiation inside a fixture binary, which it cannot.
pub(crate) fn begin_case(sink: &mut Option<Sink>) {
    if let Some(sink) = sink {
        sink.begin();
    }
}

/// Write the map after a test case, if there is a sink.
pub(crate) fn end_case(sink: &mut Option<Sink>) {
    if let Some(sink) = sink {
        sink.write();
    }
}

/// A program built without instrumentation has no counters to sink.
#[cfg(not(hegel_coverage))]
pub(crate) enum Sink {}

#[cfg(not(hegel_coverage))]
impl Sink {
    /// Never a sink: `HEGEL_FUZZ_COVERAGE` is an error for this program.
    pub(crate) fn from_env() -> Option<Sink> {
        if let Some(path) = std::env::var_os(VAR) {
            panic!(
                "{VAR}={}: this program was not built with -C instrument-coverage, so it has no \
                 coverage counters to write",
                PathBuf::from(path).display()
            );
        }
        None
    }

    pub(crate) fn begin(&mut self) {
        match *self {}
    }

    pub(crate) fn write(&mut self) {
        match *self {}
    }
}

#[cfg(test)]
#[path = "../tests/embedded/llvm_coverage_tests.rs"]
mod tests;
