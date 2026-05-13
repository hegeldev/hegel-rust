// The chrono bindings draw from `gs::dates`, `gs::times`, and related
// schemas the native backend doesn't yet implement, so this binary is
// gated to the server backend.  Phase::Target / date / time schemas
// land natively in a follow-up PR.
#![cfg(all(feature = "chrono", not(feature = "native")))]

#[path = "../common/mod.rs"]
mod common;

mod datetime;
mod duration;
mod enums;
mod naive;
mod offset;
