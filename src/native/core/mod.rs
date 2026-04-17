// Core types for the native pbtkit-style test engine.
//
// Split into submodules:
//   choices    — choice types (ChoiceKind, ChoiceNode, ChoiceValue, etc.)
//   float_index — Hypothesis float lex ordering (float_to_index, index_to_float)
//   state      — NativeTestCase, ManyState, NativeVariables, Span

mod choices;
pub mod float_index;
mod state;

#[cfg(test)]
pub use choices::{BooleanChoice, IntegerChoice};
pub use choices::{
    ChoiceKind, ChoiceNode, ChoiceValue, NodeSortKey, Status, StopTest, StringChoice,
    codepoint_key, sort_key,
};
pub use float_index::{float_to_index, index_to_float};
pub use state::{ManyState, NativeTestCase, NativeVariables, Span};

/// Maximum number of choices a single test case can make.
pub const BUFFER_SIZE: usize = 8 * 1024;

/// Maximum iterations of the outer shrink loop.
pub const MAX_SHRINK_ITERATIONS: usize = 500;

/// Probability of drawing a boundary/special value per special candidate.
pub const BOUNDARY_PROBABILITY: f64 = 0.01;
