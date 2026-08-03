//! The engine's single home for operating-system access.
//!
//! Everything the engine wants from the OS — filesystem operations for the
//! failure database, a monotonic clock for deadlines and health checks,
//! entropy for PRNG seeding, the `/dev/urandom` reader behind the urandom
//! backend, environment lookups, and stderr output — goes through this
//! module. No other module may touch the OS directly, and no other module
//! may assume an OS exists: every capability here is either fallible
//! ([`Result`]/[`Option`], which callers treat as a silent no-op or a
//! fallback) or best-effort ([`stderr_line`]), so a future no-OS backend
//! (wasm) slots in behind the same signatures.
//!
//! The per-target backends deliberately avoid process-global runtime state:
//! `rustix` on Unix (raw syscalls on Linux, so no libc thread-locals) and
//! direct `windows-sys` calls on Windows. This is part of making `libhegel`
//! safe to `dlclose` — nothing here registers TLS destructors or hooks that
//! could dangle after unload.

use core::time::Duration;

#[cfg(unix)]
#[path = "unix.rs"]
mod imp;

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

/// An OS operation failed. Carries no detail: every caller treats failure
/// as "the capability is unavailable right now" and degrades silently, so
/// there is nothing to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error;

/// Filesystem operations, all rooted at plain `&str` paths joined with
/// `/`. Failures are reported as [`Error`] without detail; the engine's
/// only filesystem client (the failure database) treats every failure as
/// a silent no-op.
pub mod fs {
    use super::{Error, imp};

    /// Names of the entries in the directory at `path`, excluding `.` and
    /// `..`. Entries whose names are not valid Unicode are skipped.
    pub fn read_dir(path: &str) -> Result<Vec<String>, Error> {
        imp::read_dir(path)
    }

    /// The full contents of the file at `path`.
    pub fn read(path: &str) -> Result<Vec<u8>, Error> {
        imp::read(path)
    }

    /// Create (or truncate) the file at `path` and write `data` to it.
    pub fn write(path: &str, data: &[u8]) -> Result<(), Error> {
        imp::write(path, data)
    }

    /// Create the directory at `path` and any missing parents.
    pub fn create_dir_all(path: &str) -> Result<(), Error> {
        let bytes = path.as_bytes();
        for i in 1..bytes.len() {
            if is_separator(bytes[i]) && !is_separator(bytes[i - 1]) {
                let _ = imp::mkdir(&path[..i]);
            }
        }
        match imp::mkdir(path) {
            Ok(()) => Ok(()),
            Err(_) if exists(path) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn is_separator(byte: u8) -> bool {
        byte == b'/' || (cfg!(windows) && byte == b'\\')
    }

    /// Atomically rename `from` to `to`, replacing `to` if it is an
    /// existing file.
    pub fn rename(from: &str, to: &str) -> Result<(), Error> {
        imp::rename(from, to)
    }

    /// Remove the file at `path`.
    pub fn remove_file(path: &str) -> Result<(), Error> {
        imp::remove_file(path)
    }

    /// Remove the directory at `path`; fails unless it is empty.
    pub fn remove_dir(path: &str) -> Result<(), Error> {
        imp::remove_dir(path)
    }

    /// Whether anything exists at `path`.
    pub fn exists(path: &str) -> bool {
        imp::exists(path)
    }
}

/// A point on the OS monotonic clock, used for deadlines and elapsed-time
/// accounting.
///
/// [`Instant::now`] returns `Option` because the clock is a capability, not
/// a given: on a platform without one, every `now` is `None`, deadlines
/// built from it stay unset, and elapsed times read as zero — timing-based
/// features (shrink deadline, TooSlow health check) quietly disable
/// themselves rather than break the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    nanos: u64,
}

impl Instant {
    /// The current point on the monotonic clock, or `None` if this platform
    /// has no monotonic clock.
    pub fn now() -> Option<Instant> {
        imp::monotonic_nanos().map(|nanos| Instant { nanos })
    }

    /// Time elapsed since this instant, saturating to zero if the clock has
    /// since become unavailable or has not advanced.
    pub fn elapsed(&self) -> Duration {
        Instant::now().map_or(Duration::ZERO, |now| now.duration_since(*self))
    }

    /// Time from `earlier` to this instant, saturating to zero if `earlier`
    /// is the later of the two.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        Duration::from_nanos(self.nanos.saturating_sub(earlier.nanos))
    }
}

impl core::ops::Add<Duration> for Instant {
    type Output = Instant;

    /// The instant `rhs` after this one, saturating at the clock's maximum
    /// representable point.
    fn add(self, rhs: Duration) -> Instant {
        Instant {
            nanos: self.nanos.saturating_add(duration_nanos(rhs)),
        }
    }
}

impl core::ops::Sub<Duration> for Instant {
    type Output = Instant;

    /// The instant `rhs` before this one, saturating at the clock's zero
    /// point.
    fn sub(self, rhs: Duration) -> Instant {
        Instant {
            nanos: self.nanos.saturating_sub(duration_nanos(rhs)),
        }
    }
}

fn duration_nanos(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

/// Fill `buf` from the OS entropy source (the `getrandom` syscall on Linux,
/// `ProcessPrng` on Windows). Callers fall back to a fixed seed on failure.
pub fn entropy(buf: &mut [u8]) -> Result<(), Error> {
    imp::entropy(buf)
}

/// Whether this platform has an OS random device (`/dev/urandom`) for the
/// urandom backend to read. When `false`, [`urandom`] always fails and the
/// backend falls back to an OS-seeded PRNG at selection time.
pub fn urandom_available() -> bool {
    imp::urandom_available()
}

/// Fill `buf` from the OS random device, opened fresh for this one read so
/// an external controller of the device (the Antithesis fuzzer) observes a
/// single read of exactly this size.
pub fn urandom(buf: &mut [u8]) -> Result<(), Error> {
    imp::urandom(buf)
}

/// The value of the environment variable `name`, decoded lossily. `None`
/// if unset.
pub fn env_var(name: &str) -> Option<String> {
    imp::env_var(name)
}

/// Write `line` plus a trailing newline to stderr, best-effort: failures
/// are ignored, and a short write may truncate the line. Diagnostics must
/// never take down a run.
pub fn stderr_line(line: &str) {
    let mut buf = Vec::with_capacity(line.len() + 1);
    buf.extend_from_slice(line.as_bytes());
    buf.push(b'\n');
    imp::stderr_write(&buf);
}

/// The current process id, used to make temporary file names unique across
/// processes sharing a database directory.
pub fn pid() -> u32 {
    imp::pid()
}

#[cfg(test)]
#[path = "../../tests/embedded/sys_tests.rs"]
mod tests;
