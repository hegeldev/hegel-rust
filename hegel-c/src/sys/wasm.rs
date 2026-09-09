//! Browser WebAssembly backend for [`crate::sys`].
//!
//! The raw module imports entropy and monotonic time from the `hegel_host`
//! WebAssembly import module. A random-fill function returns nonzero on
//! success and zero on failure. The clock returns nanoseconds, or a negative
//! value when no monotonic clock is available.

use alloc::string::String;
use core::sync::atomic::AtomicU32;

use super::Error;

const MAX_RANDOM_FILL: usize = 65_536;

#[link(wasm_import_module = "hegel_host")]
unsafe extern "C" {
    #[link_name = "entropy_fill"]
    fn host_entropy_fill(ptr: *mut u8, len: usize) -> i32;

    #[link_name = "monotonic_nanos"]
    fn host_monotonic_nanos() -> i64;
}

type HostFill = unsafe extern "C" fn(*mut u8, usize) -> i32;

fn fill_random(buf: &mut [u8], fill: HostFill) -> Result<(), Error> {
    for chunk in buf.chunks_mut(MAX_RANDOM_FILL) {
        // SAFETY: `chunk` is writable for exactly `chunk.len()` bytes. The
        // host import contract does not retain the pointer after returning.
        if unsafe { fill(chunk.as_mut_ptr(), chunk.len()) } == 0 {
            return Err(Error);
        }
    }
    Ok(())
}

pub(super) fn monotonic_nanos() -> Option<u64> {
    // SAFETY: the import takes no pointers and has no preconditions.
    let nanos = unsafe { host_monotonic_nanos() };
    u64::try_from(nanos).ok()
}

pub(super) fn entropy(buf: &mut [u8]) -> Result<(), Error> {
    fill_random(buf, host_entropy_fill)
}

pub(super) fn urandom_available() -> bool {
    false
}

pub(super) fn urandom(_buf: &mut [u8]) -> Result<(), Error> {
    Err(Error)
}

pub(super) fn env_var(_name: &str) -> Option<String> {
    None
}

pub(super) fn stderr_write(_bytes: &[u8]) {}

pub(super) fn park(_word: &AtomicU32, _expected: u32) {}

pub(super) fn unpark(_word: &AtomicU32) {}
