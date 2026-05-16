// The serde_json bindings draw from `gs::floats` and `gs::text`, which
// the native backend doesn't yet implement, so this binary is gated to
// the server backend.  The relevant schema interpreters will land in a
// follow-up PR.
#![cfg(all(feature = "serde_json", not(feature = "native")))]

#[path = "../common/mod.rs"]
mod common;

mod number;
mod value;

#[cfg(feature = "serde_json_raw_value")]
mod raw_value;
