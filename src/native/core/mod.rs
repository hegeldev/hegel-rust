// Core types for the native pbtkit-style test engine.
//
// Split into submodules:
//   choices    — choice types (ChoiceKind, ChoiceNode, ChoiceValue, etc.)
//   float_index — Hypothesis float lex ordering (float_to_index, index_to_float)
//   state      — NativeTestCase, ManyState, NativeVariables, Span

mod choices;
pub mod float_index;
mod state;

pub use choices::{
    BooleanChoice, BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, IntegerChoice,
    NodeSortKey, Status, StopTest, StringChoice, codepoint_key, sort_key,
};
pub use float_index::{
    decode_exponent, encode_exponent, float_to_index, index_to_float, reverse_bits_n,
};
pub use state::{
    CoverageTag, ManyState, NativeConjectureResult, NativeResult, NativeTestCase, NativeVariables,
    Span, structural_coverage,
};

/// Maximum number of choices a single test case can make.
pub const BUFFER_SIZE: usize = 8 * 1024;

/// Maximum iterations of the outer shrink loop.
pub const MAX_SHRINK_ITERATIONS: usize = 500;

/// Probability of drawing a boundary/special value per special candidate.
pub const BOUNDARY_PROBABILITY: f64 = 0.01;
