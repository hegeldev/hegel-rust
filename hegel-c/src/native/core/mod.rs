pub(crate) mod choices;
pub(crate) mod float_index;
pub(crate) mod state;
pub(crate) mod state_machine;
pub use choices::{
    BytesChoice, ChoiceData, ChoiceKind, ChoiceNode, ChoiceValue, ChoiceValueRef, CloneRecord,
    EngineError, FloatChoice, InterestingOrigin, NodesSortKey, RealizedStream, Status,
    StringChoice, flattened_len, flattened_values_len, sort_key,
};
pub use float_index::{float_to_index, index_to_float};
pub(crate) use state::float_clamp;
pub use state::{
    ManyState, NativeTestCase, NativeTestCaseHandle, NativeVariables, Span, SpanEvent, Spans,
};
pub use state_machine::NativeStateMachine;

/// Maximum number of choices a single test case can make.
pub const BUFFER_SIZE: usize = 8 * 1024;

/// Maximum nesting depth of cloned streams (a clone made from a clone made
/// from …). The engine rejects deeper clones the same way it rejects
/// over-deep spans, and the choice deserializer refuses deeper nesting so
/// corrupt storage can't drive unbounded recursion.
pub const MAX_CLONE_DEPTH: usize = 100;

/// Probability of drawing a boundary/special value per special candidate.
pub const BOUNDARY_PROBABILITY: f64 = 0.01;

/// Probability that an integer draw from a wide range returns one of the
/// *core* special values: the range endpoints, their inner neighbours
/// (`min + 1`, `max - 1`), zero, ±1, and the small magnitudes.
///
/// This is deliberately separate from — and applied ahead of —
/// [`BOUNDARY_PROBABILITY`], which governs the large diffuse constant pool
/// (hundreds of powers of two, factorials, primorials). That pool grows with
/// the range width, so a per-candidate weight of `BOUNDARY_PROBABILITY`
/// dilutes the handful of values a property test actually needs (endpoints,
/// zero, small magnitudes) down to well under 1% each. Giving the small core
/// set its own fixed mass keeps those values common while still leaving the
/// bulk of the probability for the middle of the range. Mirrors Hypothesis's
/// boundary/`nasty` value injection (see hypothesis#4722, hegel-rust#350).
pub const CORE_SPECIAL_PROBABILITY: f64 = 0.20;

/// Minimum span (`max_value - min_value`) at which the
/// [`CORE_SPECIAL_PROBABILITY`] tier is applied. Below this the range is
/// narrow enough that the ordinary piecewise distribution (uniform on
/// `[-256, 256]` at its core) already surfaces endpoints, zero, and small
/// magnitudes frequently, so no special-casing is needed — and leaving narrow
/// draws on their existing path keeps their generation/shrink behaviour
/// exactly as before. This matches the framing of the underlying issue: the
/// under-sampling only bites for *large or full-width* ranges.
pub const CORE_SPECIAL_MIN_SPAN: u128 = 256;

/// Hard cap on the number of successful shrink improvements per
/// counterexample. Once the shrinker has accepted this many
/// strictly-smaller candidates, further `consider` / `probe` calls
/// short-circuit so the runner doesn't get stuck chasing diminishing
/// returns on pathological inputs.
pub const MAX_SHRINKS: usize = 500;

/// Wall-clock ceiling on the whole shrinking phase. Once shrinking has run
/// for this long it stops and reports the smallest counterexample found so
/// far, rather than blocking the run indefinitely on a test whose body is
/// slow to execute (where the per-step `MAX_SHRINKS` / stall caps don't bound
/// total time). Mirrors Hypothesis's `MAX_SHRINKING_SECONDS` safety valve.
pub const MAX_SHRINKING_SECONDS: u64 = 300;
