//! The lang items a standalone `libhegel` needs to link without the Rust
//! standard library: a global allocator backed by the platform allocator and
//! a panic handler that reports the panic on stderr and aborts.
//!
//! Compiled only for `--features runtime` builds with the `std` feature
//! disabled (and never into test harnesses, which link std): builds that
//! link std anywhere in the process must use std's lang items instead, and
//! defining a second set would fail to link.
//!
//! The point of taking lang items from here instead of std is `dlclose`
//! safety: nothing in this module (or the platform bindings behind it)
//! registers a thread-local destructor, an atexit hook, or any other
//! process-global pointer into the library, so unloading `libhegel` leaves
//! nothing behind to dangle. The panic handler aborts rather than unwinding
//! because there is no unwinder without std — build these artifacts with
//! `-C panic=abort` — and because the engine treats any panic as an internal
//! bug for which "report and abort" is the contract.

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;

use super::imp;

/// The platform allocator (`malloc`/`posix_memalign` on Unix, `HeapAlloc`
/// on Windows), exposed as Rust's global allocator.
struct PlatformAllocator;

// SAFETY: the platform heap never unmaps or moves live allocations, `alloc`
// satisfies the requested layout or returns null, and `dealloc` returns the
// block to the same heap it came from.
unsafe impl GlobalAlloc for PlatformAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        imp::alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `GlobalAlloc` guarantees `ptr` came from `alloc` with this
        // `layout` and has not been freed, which is `imp::dealloc`'s contract.
        unsafe { imp::dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: PlatformAllocator = PlatformAllocator;

/// A `core::fmt::Write` sink into a fixed stack buffer that silently drops
/// whatever does not fit, so the panic handler can format the panic message
/// without touching the (possibly wedged) allocator.
struct TruncatingBuffer {
    buf: [u8; 512],
    len: usize,
}

impl Write for TruncatingBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let room = self.buf.len() - self.len;
        let take = s.len().min(room);
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

#[cfg(all(unix, not(target_vendor = "apple")))]
core::arch::global_asm!(".hidden rust_eh_personality", ".hidden _Unwind_Resume");

/// Satisfy the unwinding references the precompiled `core`/`alloc` carry
/// (they are built with `panic = "unwind"`), so the shared library has no
/// undefined unwinder symbols and loads under `RTLD_NOW`. Never called:
/// these builds use `-C panic=abort` and the panic handler aborts, so no
/// unwind can start. Aborts if something calls it anyway.
///
/// Both stubs are given hidden visibility on ELF targets (the
/// `global_asm!` `.hidden` directives above) so a library loaded with
/// `RTLD_GLOBAL` cannot interpose them over a real unwinder in another
/// shared object, and so no other object's PLT can bind to them and dangle
/// after `dlclose`; the dynamic symbol table then exports only `hegel_*`.
#[unsafe(no_mangle)]
extern "C" fn rust_eh_personality() -> ! {
    imp::abort_process()
}

/// See [`rust_eh_personality`]: the `_Unwind_Resume` reference normally
/// resolved from libgcc, satisfied locally so the library stands alone.
/// Never called; aborts if something calls it anyway.
#[unsafe(no_mangle)]
extern "C" fn _Unwind_Resume() -> ! {
    imp::abort_process()
}

/// Report the panic on stderr and abort the process. Reaching this is a bug
/// in libhegel: the engine has no deliberate panics, so the message asks for
/// a report rather than unwinding garbage state into the host.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let mut out = TruncatingBuffer {
        buf: [0; 512],
        len: 0,
    };
    let _ = write!(out, "libhegel internal error (please file a bug): {info}");
    imp::stderr_write(&out.buf[..out.len]);
    imp::stderr_write(b"\n");
    imp::abort_process()
}
