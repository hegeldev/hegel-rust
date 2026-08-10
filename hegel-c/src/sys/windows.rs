//! Windows backend for [`crate::sys`], built on `windows-sys` (direct
//! kernel32 / bcryptprimitives calls, no C runtime state).

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU32;

use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    SetLastError,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_ALWAYS, CreateDirectoryW, CreateFileW, DeleteFileW, FILE_ATTRIBUTE_NORMAL,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FindClose, FindFirstFileW, FindNextFileW,
    GetFileAttributesW, INVALID_FILE_ATTRIBUTES, MOVEFILE_REPLACE_EXISTING, MoveFileExW,
    OPEN_EXISTING, ReadFile, RemoveDirectoryW, WIN32_FIND_DATAW, WriteFile,
};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE};

use super::Error;

/// `path` encoded as a NUL-terminated UTF-16 string.
fn wide(path: &str) -> Vec<u16> {
    path.encode_utf16().chain(core::iter::once(0)).collect()
}

/// An owned file handle that closes itself on drop.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid handle owned by this wrapper.
        unsafe { CloseHandle(self.0) };
    }
}

fn open(path: &str, access: u32, share: u32, disposition: u32) -> Result<OwnedHandle, Error> {
    let wpath = wide(path);
    // SAFETY: `wpath` is NUL-terminated and outlives the call.
    let handle = unsafe {
        CreateFileW(
            wpath.as_ptr(),
            access,
            share,
            core::ptr::null(),
            disposition,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(Error);
    }
    Ok(OwnedHandle(handle))
}

/// Names of the entries in the directory at `path`, excluding `.` and `..`.
/// Entries whose names are not valid UTF-16 are skipped.
pub(super) fn read_dir(path: &str) -> Result<Vec<String>, Error> {
    let pattern = wide(&format!("{path}/*"));
    // SAFETY: `data` is a valid out-pointer and `pattern` is NUL-terminated.
    let mut data: WIN32_FIND_DATAW = unsafe { core::mem::zeroed() };
    let find = unsafe { FindFirstFileW(pattern.as_ptr(), &mut data) };
    if find == INVALID_HANDLE_VALUE {
        return Err(Error);
    }
    let mut names = Vec::new();
    loop {
        let len = data
            .cFileName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(data.cFileName.len());
        if let Ok(name) = String::from_utf16(&data.cFileName[..len]) {
            if name != "." && name != ".." {
                names.push(name);
            }
        }
        // SAFETY: `find` is a valid search handle and `data` a valid out-pointer.
        if unsafe { FindNextFileW(find, &mut data) } == 0 {
            break;
        }
    }
    // SAFETY: `find` is a valid search handle not used after this call.
    unsafe { FindClose(find) };
    Ok(names)
}

/// The full contents of the file at `path`.
pub(super) fn read(path: &str) -> Result<Vec<u8>, Error> {
    let handle = open(
        path,
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        OPEN_EXISTING,
    )?;
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let mut n: u32 = 0;
        // SAFETY: `chunk` and `n` are valid for writes of the stated sizes.
        let ok = unsafe {
            ReadFile(
                handle.0,
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                &mut n,
                core::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(Error);
        }
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&chunk[..n as usize]);
    }
}

/// Create (or truncate) the file at `path` and write `data` to it.
pub(super) fn write(path: &str, data: &[u8]) -> Result<(), Error> {
    let handle = open(path, GENERIC_WRITE, 0, CREATE_ALWAYS)?;
    let mut remaining = data;
    while !remaining.is_empty() {
        let mut n: u32 = 0;
        // SAFETY: `remaining` is valid for reads of its length and `n` for a write.
        let ok = unsafe {
            WriteFile(
                handle.0,
                remaining.as_ptr(),
                remaining.len() as u32,
                &mut n,
                core::ptr::null_mut(),
            )
        };
        if ok == 0 || n == 0 {
            return Err(Error);
        }
        remaining = &remaining[n as usize..];
    }
    Ok(())
}

/// Create a single directory level at `path`.
pub(super) fn mkdir(path: &str) -> Result<(), Error> {
    let wpath = wide(path);
    // SAFETY: `wpath` is NUL-terminated and outlives the call.
    if unsafe { CreateDirectoryW(wpath.as_ptr(), core::ptr::null()) } == 0 {
        return Err(Error);
    }
    Ok(())
}

/// Atomically rename `from` to `to`, replacing `to` if it is an existing
/// file.
pub(super) fn rename(from: &str, to: &str) -> Result<(), Error> {
    let wfrom = wide(from);
    let wto = wide(to);
    // SAFETY: both strings are NUL-terminated and outlive the call.
    if unsafe { MoveFileExW(wfrom.as_ptr(), wto.as_ptr(), MOVEFILE_REPLACE_EXISTING) } == 0 {
        return Err(Error);
    }
    Ok(())
}

/// Remove the file at `path`.
pub(super) fn remove_file(path: &str) -> Result<(), Error> {
    let wpath = wide(path);
    // SAFETY: `wpath` is NUL-terminated and outlives the call.
    if unsafe { DeleteFileW(wpath.as_ptr()) } == 0 {
        return Err(Error);
    }
    Ok(())
}

/// Remove the directory at `path`; fails unless it is empty.
pub(super) fn remove_dir(path: &str) -> Result<(), Error> {
    let wpath = wide(path);
    // SAFETY: `wpath` is NUL-terminated and outlives the call.
    if unsafe { RemoveDirectoryW(wpath.as_ptr()) } == 0 {
        return Err(Error);
    }
    Ok(())
}

/// Whether anything exists at `path`.
pub(super) fn exists(path: &str) -> bool {
    let wpath = wide(path);
    // SAFETY: `wpath` is NUL-terminated and outlives the call.
    let attributes = unsafe { GetFileAttributesW(wpath.as_ptr()) };
    attributes != INVALID_FILE_ATTRIBUTES
}

/// Nanoseconds on the monotonic clock (`QueryPerformanceCounter`).
pub(super) fn monotonic_nanos() -> Option<u64> {
    let mut counter: i64 = 0;
    let mut frequency: i64 = 0;
    // SAFETY: both out-pointers are valid for writes.
    let ok = unsafe {
        windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut counter) != 0
            && windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency)
                != 0
    };
    if !ok || frequency <= 0 {
        return None;
    }
    let nanos = (counter as u128).saturating_mul(1_000_000_000) / frequency as u128;
    Some(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// Fill `buf` from the OS entropy source (`ProcessPrng`).
pub(super) fn entropy(buf: &mut [u8]) -> Result<(), Error> {
    // SAFETY: `buf` is valid for writes of its length.
    let ok = unsafe {
        windows_sys::Win32::Security::Cryptography::ProcessPrng(buf.as_mut_ptr(), buf.len())
    };
    if ok == 0 { Err(Error) } else { Ok(()) }
}

/// Whether this platform has an OS random device for the urandom backend.
/// Windows has no `/dev/urandom` equivalent an external controller could
/// hook, so the urandom backend is unavailable.
pub(super) fn urandom_available() -> bool {
    false
}

/// Always fails: there is no OS random device on Windows.
pub(super) fn urandom(_buf: &mut [u8]) -> Result<(), Error> {
    Err(Error)
}

/// Best-effort single `WriteFile` of `bytes` to stderr; failures and short
/// writes are ignored.
pub(super) fn stderr_write(bytes: &[u8]) {
    // SAFETY: `GetStdHandle` returns a process-lifetime handle (or an
    // invalid one, which `WriteFile` rejects); `bytes` is valid for reads.
    unsafe {
        let handle = GetStdHandle(STD_ERROR_HANDLE);
        let mut n: u32 = 0;
        WriteFile(
            handle,
            bytes.as_ptr(),
            bytes.len() as u32,
            &mut n,
            core::ptr::null_mut(),
        );
    }
}

/// The value of the environment variable `name`. `None` if unset; a
/// variable set to the empty string is `Some("")`, distinguished from
/// "unset" via `GetLastError` as `std` does.
pub(super) fn env_var(name: &str) -> Option<String> {
    let wname = wide(name);
    let mut buf = vec![0u16; 256];
    loop {
        // SAFETY: `wname` is NUL-terminated and `buf` is valid for writes
        // of its length.
        let n = unsafe {
            SetLastError(0);
            windows_sys::Win32::System::Environment::GetEnvironmentVariableW(
                wname.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };
        if n == 0 {
            // SAFETY: no preconditions.
            let empty_but_set = unsafe { GetLastError() } == 0;
            return if empty_but_set {
                Some(String::new())
            } else {
                None
            };
        }
        if (n as usize) < buf.len() {
            return Some(String::from_utf16_lossy(&buf[..n as usize]));
        }
        buf.resize(n as usize, 0);
    }
}

/// The current process id.
pub(super) fn pid() -> u32 {
    // SAFETY: no preconditions.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() }
}

/// Block until [`unpark`] is called on `word`, returning immediately (and
/// possibly spuriously) if `word` no longer holds `expected`.
pub(super) fn park(word: &AtomicU32, expected: u32) {
    // SAFETY: both addresses are valid for reads of four bytes for the
    // duration of the call, which is the size passed.
    unsafe {
        windows_sys::Win32::System::Threading::WaitOnAddress(
            word.as_ptr().cast(),
            (&raw const expected).cast(),
            size_of::<u32>(),
            windows_sys::Win32::System::Threading::INFINITE,
        );
    }
}

/// Wake one thread parked on `word` by [`park`].
pub(super) fn unpark(word: &AtomicU32) {
    // SAFETY: `word` is a valid address to signal on.
    unsafe { windows_sys::Win32::System::Threading::WakeByAddressSingle(word.as_ptr().cast()) };
}

/// The alignment `HeapAlloc` guarantees (`MEMORY_ALLOCATION_ALIGNMENT`):
/// twice the pointer size, so 16 on 64-bit Windows and 8 on 32-bit.
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
const HEAP_ALIGN: usize = 2 * size_of::<usize>();

/// Allocate `layout.size()` bytes at `layout.align()` alignment from the
/// process heap. Alignments beyond `HeapAlloc`'s guarantee are met by
/// over-allocating and stashing the raw pointer in the `usize` slot just
/// below the aligned block, where [`dealloc`] recovers it. Returns null on
/// failure. `layout` must have non-zero size (the `GlobalAlloc` contract).
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
pub(super) fn alloc(layout: core::alloc::Layout) -> *mut u8 {
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapAlloc};
    // SAFETY: `GetProcessHeap` has no preconditions, and `HeapAlloc` on the
    // process heap with no flags is valid for any size.
    unsafe {
        let heap = GetProcessHeap();
        if layout.align() <= HEAP_ALIGN {
            HeapAlloc(heap, 0, layout.size()).cast()
        } else {
            let raw: *mut u8 = HeapAlloc(heap, 0, layout.size() + layout.align()).cast();
            if raw.is_null() {
                return raw;
            }
            let aligned = raw.add(layout.align() - (raw as usize & (layout.align() - 1)));
            aligned.cast::<usize>().sub(1).write_unaligned(raw as usize);
            aligned
        }
    }
}

/// Return an allocation made by [`alloc`] to the process heap.
///
/// # Safety
///
/// `ptr` must have been returned by [`alloc`] with an equivalent `layout`
/// and not yet deallocated.
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
pub(super) unsafe fn dealloc(ptr: *mut u8, layout: core::alloc::Layout) {
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapFree};
    // SAFETY: the caller guarantees `ptr` is a live [`alloc`] result, so for
    // an over-aligned layout the `usize` below it holds the raw `HeapAlloc`
    // pointer, and freeing that pointer on the process heap is valid.
    unsafe {
        let raw = if layout.align() <= HEAP_ALIGN {
            ptr
        } else {
            ptr.cast::<usize>().sub(1).read_unaligned() as *mut u8
        };
        HeapFree(GetProcessHeap(), 0, raw.cast());
    }
}

/// Abort the process without unwinding or running any cleanup.
#[cfg(all(feature = "runtime", not(feature = "std"), not(test)))]
pub(super) fn abort_process() -> ! {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
    // SAFETY: terminating the current process has no memory-safety
    // preconditions.
    unsafe { TerminateProcess(GetCurrentProcess(), 3) };
    loop {}
}
