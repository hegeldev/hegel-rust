//! Unix backend for [`crate::sys`], built on `rustix` (raw syscalls on
//! Linux, so no libc thread-local state) plus a direct `getenv` declaration
//! for environment lookups.

use core::sync::atomic::AtomicU32;

use rustix::fs::{Mode, OFlags};

use super::Error;

impl From<rustix::io::Errno> for Error {
    fn from(_: rustix::io::Errno) -> Error {
        Error
    }
}

/// Call `op` until it returns anything other than `EINTR`, so a signal
/// delivered mid-syscall restarts the operation instead of failing it.
pub(super) fn retry_intr<T>(
    mut op: impl FnMut() -> Result<T, rustix::io::Errno>,
) -> Result<T, rustix::io::Errno> {
    loop {
        match op() {
            Err(e) if e == rustix::io::Errno::INTR => {}
            other => return other,
        }
    }
}

/// Names of the entries in the directory at `path`, excluding `.` and `..`.
/// Entries whose names are not valid UTF-8 are skipped.
pub(super) fn read_dir(path: &str) -> Result<Vec<String>, Error> {
    let fd = retry_intr(|| {
        rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
    })?;
    let dir = rustix::fs::Dir::read_from(&fd)?;
    let mut names = Vec::new();
    for entry in dir {
        let entry = entry?;
        if let Ok(name) = entry.file_name().to_str() {
            if name != "." && name != ".." {
                names.push(name.to_owned());
            }
        }
    }
    Ok(names)
}

/// The full contents of the file at `path`.
pub(super) fn read(path: &str) -> Result<Vec<u8>, Error> {
    let fd = retry_intr(|| rustix::fs::open(path, OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty()))?;
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = retry_intr(|| rustix::io::read(&fd, &mut chunk[..]))?;
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&chunk[..n]);
    }
}

/// Read exactly `buf.len()` bytes from the start of the file at `path`.
/// Fails if the file is shorter than the buffer.
pub(super) fn read_exact(path: &str, buf: &mut [u8]) -> Result<(), Error> {
    let fd = retry_intr(|| rustix::fs::open(path, OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty()))?;
    let mut filled = 0;
    while filled < buf.len() {
        let n = retry_intr(|| rustix::io::read(&fd, &mut buf[filled..]))?;
        if n == 0 {
            return Err(Error);
        }
        filled += n;
    }
    Ok(())
}

/// Create (or truncate) the file at `path` and write `data` to it.
pub(super) fn write(path: &str, data: &[u8]) -> Result<(), Error> {
    let fd = retry_intr(|| {
        rustix::fs::open(
            path,
            OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o666),
        )
    })?;
    let mut remaining = data;
    while !remaining.is_empty() {
        let n = retry_intr(|| rustix::io::write(&fd, remaining))?;
        remaining = &remaining[n..];
    }
    Ok(())
}

/// Create a single directory level at `path`.
pub(super) fn mkdir(path: &str) -> Result<(), Error> {
    rustix::fs::mkdir(path, Mode::from_bits_truncate(0o777))?;
    Ok(())
}

/// Atomically rename `from` to `to`, replacing `to` if it is an existing
/// file.
pub(super) fn rename(from: &str, to: &str) -> Result<(), Error> {
    rustix::fs::rename(from, to)?;
    Ok(())
}

/// Remove the file at `path`.
pub(super) fn remove_file(path: &str) -> Result<(), Error> {
    rustix::fs::unlink(path)?;
    Ok(())
}

/// Remove the directory at `path`; fails unless it is empty.
pub(super) fn remove_dir(path: &str) -> Result<(), Error> {
    rustix::fs::rmdir(path)?;
    Ok(())
}

/// Whether anything exists at `path`.
pub(super) fn exists(path: &str) -> bool {
    rustix::fs::stat(path).is_ok()
}

/// Nanoseconds on the monotonic clock (`CLOCK_MONOTONIC`).
pub(super) fn monotonic_nanos() -> Option<u64> {
    let ts = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Some(
        (ts.tv_sec as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(ts.tv_nsec as u64),
    )
}

/// Fill `buf` from the OS entropy source: the `getrandom` syscall on Linux,
/// a `/dev/urandom` read elsewhere.
#[cfg(target_os = "linux")]
pub(super) fn entropy(buf: &mut [u8]) -> Result<(), Error> {
    let n = rustix::rand::getrandom(&mut *buf, rustix::rand::GetRandomFlags::empty())?;
    if n == buf.len() { Ok(()) } else { Err(Error) }
}

/// Fill `buf` from the OS entropy source: the `getrandom` syscall on Linux,
/// a `/dev/urandom` read elsewhere.
#[cfg(not(target_os = "linux"))]
pub(super) fn entropy(buf: &mut [u8]) -> Result<(), Error> {
    urandom(buf)
}

/// Whether this platform has an OS random device for the urandom backend.
pub(super) fn urandom_available() -> bool {
    true
}

/// Fill `buf` from `/dev/urandom`, opening it fresh for this one read so an
/// external controller of the random device observes a single read of
/// exactly this size.
pub(super) fn urandom(buf: &mut [u8]) -> Result<(), Error> {
    read_exact("/dev/urandom", buf)
}

/// Best-effort single `write` of `bytes` to stderr; failures and short
/// writes are ignored.
pub(super) fn stderr_write(bytes: &[u8]) {
    let _ = rustix::io::write(rustix::stdio::stderr(), bytes);
}

unsafe extern "C" {
    fn getenv(name: *const core::ffi::c_char) -> *const core::ffi::c_char;
}

/// The value of the environment variable `name`, decoded lossily from
/// whatever bytes the environment holds. `None` if unset or if `name`
/// contains an interior NUL.
pub(super) fn env_var(name: &str) -> Option<String> {
    if name.as_bytes().contains(&0) {
        return None;
    }
    let mut cname = Vec::with_capacity(name.len() + 1);
    cname.extend_from_slice(name.as_bytes());
    cname.push(0);
    // SAFETY: `cname` is NUL-terminated and outlives the call.
    let ptr = unsafe { getenv(cname.as_ptr().cast()) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: a non-null `getenv` result points at a NUL-terminated string
    // that lives as long as the environment entry.
    let bytes = unsafe { core::ffi::CStr::from_ptr(ptr) }.to_bytes();
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// The current process id.
pub(super) fn pid() -> u32 {
    rustix::process::getpid().as_raw_nonzero().get() as u32
}

/// Block until [`unpark`] is called on `word`, returning immediately (and
/// possibly spuriously) if `word` no longer holds `expected`.
///
/// Linux has the futex syscall for exactly this. Every other Unix spins:
/// it yields the CPU and lets the caller re-check, which is correct — the
/// caller must tolerate spurious returns anyway — but burns time under
/// contention. The engine's locks are essentially always uncontended, so
/// this costs nothing in practice.
#[cfg(target_os = "linux")]
pub(super) fn park(word: &AtomicU32, expected: u32) {
    let _ =
        rustix::thread::futex::wait(word, rustix::thread::futex::Flags::PRIVATE, expected, None);
}

/// Wake one thread parked on `word` by [`park`].
#[cfg(target_os = "linux")]
pub(super) fn unpark(word: &AtomicU32) {
    let _ = rustix::thread::futex::wake(word, rustix::thread::futex::Flags::PRIVATE, 1);
}

/// Yield the CPU so a thread waiting on `word` makes progress; see the
/// Linux [`park`] for the contract.
#[cfg(not(target_os = "linux"))]
pub(super) fn park(_word: &AtomicU32, _expected: u32) {
    rustix::thread::sched_yield();
}

/// No-op: the non-Linux [`park`] spins rather than sleeping, so there is
/// nobody to wake.
#[cfg(not(target_os = "linux"))]
pub(super) fn unpark(_word: &AtomicU32) {}
