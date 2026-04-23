//! Tests ported from pbtkit/tests/. Add one `mod <name>;` per ported file,
//! alphabetical.

#[path = "../common/mod.rs"]
mod common;

mod bytes;
mod choice_index;
mod core;
mod draw_names;
mod findability_arithmetic;
mod findability_collections;
mod findability_floats;
mod findability_pbtsmith_regressions;
mod findability_types;
mod floats;
mod generators;
mod shrink_quality_bytes;
mod shrink_quality_collections;
mod shrink_quality_composite;
mod shrink_quality_flatmap;
mod shrink_quality_integers;
mod shrink_quality_mixed_types;
mod shrink_quality_strings;
mod spans;
mod text;
