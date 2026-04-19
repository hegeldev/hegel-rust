//! Tests ported from hypothesis-python/tests/cover/. Add one `mod <name>;`
//! per ported file, alphabetical.

#[path = "../common/mod.rs"]
mod common;

mod composite;
mod composite_kwonlyargs;
mod database_backend;
mod datetimes;
mod float_nastiness;
mod regex;
mod replay_logic;
mod testdecorators;
mod threading;
mod uuids;
mod verbosity;
