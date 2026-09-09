//! The `hegel_*` C ABI as seen by the frontend.
//!
//! Everything the frontend knows about libhegel comes through this module:
//! the opaque handle types, the `repr(C)` enums and structs, and the
//! `hegel_*` functions themselves. It has two interchangeable forms. By
//! default the engine is a separately built `libhegel_c` shared library:
//! [`types`] mirrors the `repr(C)` types and [`loader`] resolves each
//! function out of the library at runtime, so the `hegeltest-c` crate — and
//! its whole dependency tree — never appears in a consumer's cargo graph.
//! With the `static-engine` feature the module is instead a re-export of the
//! `hegel_c` rlib, linking the engine into the binary like any other Rust
//! dependency.
//!
//! Either way the surface is identical, down to every function being
//! `unsafe`: the one safe C function, `hegel_context_new`, is wrapped so
//! call sites cannot tell the modes apart. [`for_each_hegel_fn`] is the
//! single list of functions both forms are generated from, itself generated
//! from the engine source by `scripts/gen-ffi-list.py` so it is the whole
//! exported ABI rather than whatever this frontend happens to call; the
//! embedded tests compile-assert it, and the mirrored types, against the
//! real `hegel_c` definitions whenever `static-engine` makes both visible.

#[cfg_attr(feature = "static-engine", allow(unused_macros, unused_imports))]
mod fns;
#[cfg_attr(feature = "static-engine", allow(unused_imports))]
pub(crate) use fns::for_each_hegel_fn;

#[cfg(feature = "static-engine")]
pub(crate) use hegel_c::*;

#[cfg(feature = "static-engine")]
pub(crate) unsafe fn hegel_context_new() -> *mut HegelContext {
    hegel_c::hegel_context_new()
}

#[cfg(not(feature = "static-engine"))]
mod loader;
#[cfg(any(not(feature = "static-engine"), test))]
mod types;

#[cfg(not(feature = "static-engine"))]
pub(crate) use loader::*;
#[cfg(not(feature = "static-engine"))]
pub(crate) use types::*;

#[cfg(test)]
#[path = "../../tests/embedded/ffi/sys_tests.rs"]
mod tests;
