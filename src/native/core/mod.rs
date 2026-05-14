// Core types for the native Hypothesis-style test engine.
//
// Split into submodules:
//   choices    — choice types (ChoiceKind, ChoiceNode, ChoiceValue, etc.)
//   state      — NativeTestCase, ManyState, NativeVariables, Span

pub(crate) mod choices;
pub(crate) mod state;
pub use choices::{ChoiceKind, ChoiceNode, ChoiceValue, NodeSortKey, Status, StopTest, sort_key};
pub use state::{ManyState, NativeTestCase, NativeVariables, Span};

/// Maximum number of choices a single test case can make.
pub const BUFFER_SIZE: usize = 8 * 1024;

/// Maximum iterations of the outer shrink loop.
pub const MAX_SHRINK_ITERATIONS: usize = 500;

/// Probability of drawing a boundary/special value per special candidate.
pub const BOUNDARY_PROBABILITY: f64 = 0.01;
