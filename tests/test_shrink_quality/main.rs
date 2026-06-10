//! Tests asserting that the shrinker produces minimal counterexamples for
//! various generator shapes. Top-level binary mirroring the source pbtkit
//! organisation (one sub-module per topic).

#[path = "../common/mod.rs"]
mod common;

mod bytes;
mod collections;
mod composite;
mod flatmap;
mod floats;
mod integers;
mod mixed_types;
mod strings;
