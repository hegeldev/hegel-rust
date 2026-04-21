//! Tests ported from hypothesis-python/tests/cover/. Add one `mod <name>;`
//! per ported file, alphabetical.

#[path = "../common/mod.rs"]
mod common;

mod composite;
mod composite_kwonlyargs;
mod database_backend;
mod datetimes;
mod debug_information;
mod direct_strategies;
mod draw_example;
mod feature_flags;
mod find;
mod float_nastiness;
mod health_checks;
mod one_of;
mod permutations;
mod regex;
mod replay_logic;
mod sampled_from;
mod searchstrategy;
mod simple_strings;
mod testdecorators;
mod threading;
mod uuids;
mod verbosity;
