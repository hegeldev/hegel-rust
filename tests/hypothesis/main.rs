//! Tests ported from hypothesis-python/tests/cover/. Add one `mod <name>;`
//! per ported file, alphabetical.

#[path = "../common/mod.rs"]
mod common;

mod composite;
mod composite_kwonlyargs;
mod database_backend;
mod datetimes;
mod draw_example;
mod debug_information;
mod feature_flags;
mod float_nastiness;
mod permutations;
mod regex;
mod replay_logic;
mod sampled_from;
mod testdecorators;
mod threading;
mod uuids;
mod verbosity;
