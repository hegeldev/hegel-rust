// Core types for the native test engine.
//
// Split into submodules:
//   choices     — choice types (ChoiceKind, ChoiceNode, ChoiceValue, etc.)
//   float_index — float lex ordering (float_to_index, index_to_float)
//   state       — NativeTestCase, ManyState, NativeVariables, Span

pub(crate) mod choices;
pub(crate) mod float_index;
pub(crate) mod state;
pub use choices::{
    BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, NodeSortKey, NodesSortKey,
    Status, StopTest, StringChoice, sort_key,
};
pub use float_index::{float_to_index, index_to_float};
pub use state::{ManyState, NativeTestCase, NativeVariables, Span, Spans};

/// Maximum number of choices a single test case can make.
pub const BUFFER_SIZE: usize = 8 * 1024;

/// Probability of drawing a boundary/special value per special candidate.
pub const BOUNDARY_PROBABILITY: f64 = 0.01;

/// Hard cap on the number of successful shrink improvements per
/// counterexample. Once the shrinker has accepted this many
/// strictly-smaller candidates, further `consider` / `probe` calls
/// short-circuit so the runner doesn't get stuck chasing diminishing
/// returns on pathological inputs.
pub const MAX_SHRINKS: usize = 500;
