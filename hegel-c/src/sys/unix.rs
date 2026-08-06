//! Unix backend for [`crate::sys`], built on `rustix` (raw syscalls on
//! Linux, so no libc thread-local state) plus a direct `getenv` declaration
//! for environment lookups.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
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
    let fd =
        retry_intr(|| rustix::fs::open(path, OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty()))?;
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

/// Fill all of `buf` by calling `fill` on the unfilled tail until it is
/// complete, retrying `EINTR` and resuming after short fills. Fails if
/// `fill` ever succeeds with zero bytes of progress.
pub(super) fn fill_all(
    buf: &mut [u8],
    mut fill: impl FnMut(&mut [u8]) -> Result<usize, rustix::io::Errno>,
) -> Result<(), Error> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = retry_intr(|| fill(&mut buf[filled..]))?;
        if n == 0 {
            return Err(Error);
        }
        filled += n;
    }
    Ok(())
}

/// Write all of `data` by calling `write` on the unwritten tail until it is
/// consumed, retrying `EINTR` and resuming after short writes. Fails if
/// `write` ever succeeds with zero bytes of progress.
pub(super) fn write_all(
    mut data: &[u8],
    mut write: impl FnMut(&[u8]) -> Result<usize, rustix::io::Errno>,
) -> Result<(), Error> {
    while !data.is_empty() {
        let n = retry_intr(|| write(data))?;
        if n == 0 {
            return Err(Error);
        }
        data = &data[n..];
    }
    Ok(())
}

/// Read exactly `buf.len()` bytes from the start of the file at `path`.
/// Fails if the file is shorter than the buffer.
pub(super) fn read_exact(path: &str, buf: &mut [u8]) -> Result<(), Error> {
    let fd =
        retry_intr(|| rustix::fs::open(path, OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty()))?;
    fill_all(buf, |chunk| rustix::io::read(&fd, chunk))
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
    write_all(data, |chunk| rustix::io::write(&fd, chunk))
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
    fill_all(buf, |chunk| {
        rustix::rand::getrandom(chunk, rustix::rand::GetRandomFlags::empty())
    })
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
    // SAFETY: nothing in libhegel closes fd 2, and a host process that does
    // so accepts misdirected diagnostics from every library it loaded; this
    // write is best-effort output on a fd we never retain.
    let fd = unsafe { rustix::fd::BorrowedFd::borrow_raw(rustix::stdio::raw_stderr()) };
    let _ = rustix::io::write(fd, bytes);
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

#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
unsafe extern "C" {
    fn malloc(size: usize) -> *mut core::ffi::c_void;
    fn free(ptr: *mut core::ffi::c_void);
    fn posix_memalign(
        memptr: *mut *mut core::ffi::c_void,
        alignment: usize,
        size: usize,
    ) -> core::ffi::c_int;
    fn abort() -> !;
}

/// The alignment `malloc` guarantees for any allocation at least that large
/// (`max_align_t`): 16 on 64-bit platforms, 8 on 32-bit ones.
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
const MALLOC_ALIGN: usize = if size_of::<usize>() == 8 { 16 } else { 8 };

/// Allocate `layout.size()` bytes at `layout.align()` alignment from the C
/// heap, via plain `malloc` when its guaranteed alignment suffices and
/// `posix_memalign` otherwise. Returns null on failure. `layout` must have
/// non-zero size (the `GlobalAlloc` contract).
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
pub(super) fn alloc(layout: core::alloc::Layout) -> *mut u8 {
    if layout.align() <= MALLOC_ALIGN && layout.align() <= layout.size() {
        // SAFETY: `malloc` has no preconditions.
        unsafe { malloc(layout.size()).cast() }
    } else {
        let mut out: *mut core::ffi::c_void = core::ptr::null_mut();
        let align = layout.align().max(size_of::<*const core::ffi::c_void>());
        // SAFETY: `out` is a valid pointer to write the allocation through,
        // and `align` is a power of two at least `sizeof(void *)` as
        // `posix_memalign` requires.
        let rc = unsafe { posix_memalign(&mut out, align, layout.size()) };
        if rc == 0 {
            out.cast()
        } else {
            core::ptr::null_mut()
        }
    }
}

/// Return an allocation made by [`alloc`] to the C heap. Both `malloc` and
/// `posix_memalign` results are freed with plain `free`, so the layout is
/// not needed here (unlike on Windows).
///
/// # Safety
///
/// `ptr` must have been returned by [`alloc`] and not yet deallocated.
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
pub(super) unsafe fn dealloc(ptr: *mut u8, _layout: core::alloc::Layout) {
    // SAFETY: both `malloc` and `posix_memalign` results are freed with
    // `free`, and the caller guarantees `ptr` is such a live result.
    unsafe { free(ptr.cast()) }
}

/// Abort the process without unwinding or running any cleanup.
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
pub(super) fn abort_process() -> ! {
    // SAFETY: `abort` has no preconditions.
    unsafe { abort() }
}
