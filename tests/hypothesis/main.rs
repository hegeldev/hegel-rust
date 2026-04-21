//! Tests ported from hypothesis-python/tests/cover/. Add one `mod <name>;`
//! per ported file, alphabetical.

#[path = "../common/mod.rs"]
mod common;

mod arbitrary_data;
mod cache_implementation;
mod composite;
mod composite_kwonlyargs;
mod database_backend;
mod datetimes;
mod debug_information;
mod direct_strategies;
mod draw_example;
mod feature_flags;
mod filtered_strategy;
mod find;
mod float_nastiness;
mod float_utils;
mod given_error_conditions;
mod health_checks;
mod intervalset;
mod numerics;
mod one_of;
mod permutations;
mod regex;
mod replay_logic;
mod sampled_from;
mod searchstrategy;
mod shrink_budgeting;
mod simple_characters;
mod simple_collections;
mod simple_strings;
mod testdecorators;
mod threading;
mod uuids;
mod verbosity;
