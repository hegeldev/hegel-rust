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
    BytesChoice, ChoiceKind, ChoiceNode, ChoiceValue, EngineError, FloatChoice, NodesSortKey,
    Status, StringChoice, sort_key,
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

/// Stall budget for the shrinker — Hypothesis's `max_stall`: once this many
/// test-function calls have been made since the last accepted shrink, the
/// whole shrink ends. Grows during a run (on every accepted shrink and with
/// fixate's per-iteration padding), so this is only the starting value.
pub const MAX_STALL: usize = 200;

/// Wall-clock ceiling on the whole shrinking phase. Once shrinking has run
/// for this long it stops and reports the smallest counterexample found so
/// far, rather than blocking the run indefinitely on a test whose body is
/// slow to execute (where the per-step `MAX_SHRINKS` / stall caps don't bound
/// total time). Mirrors Hypothesis's `MAX_SHRINKING_SECONDS` safety valve.
pub const MAX_SHRINKING_SECONDS: u64 = 300;
