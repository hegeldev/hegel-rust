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
pub use state::{
    GenerationParameters, ManyState, NativeTestCase, NativeTestCaseHandle, NativeVariables, Span,
    SpanEvent, Spans,
};
pub use state_machine::NativeStateMachine;

/// Maximum number of choices a single test case can make.
pub const BUFFER_SIZE: usize = 8 * 1024;

/// Maximum nesting depth of cloned streams (a clone made from a clone made
/// from …). The engine rejects deeper clones the same way it rejects
/// over-deep spans, and the choice deserializer refuses deeper nesting so
/// corrupt storage can't drive unbounded recursion.
pub const MAX_CLONE_DEPTH: usize = 100;

/// Probability of drawing a boundary/special value per special candidate. Used
/// by the narrow-range, float, string and bytes samplers (the wide-range integer
/// sampler uses the per-category Dirichlet weights below instead).
pub const BOUNDARY_PROBABILITY: f64 = 0.01;

/// How a wide-range integer draw is split between four *categories* of value is
/// not fixed: a mixture weight for each category is drawn afresh for every test
/// case (as that case's [`GenerationParameters`]) from a Dirichlet distribution,
/// a lumpy form of *swarm testing*. The categories are:
///
///   * **endpoints** — the range edges `{min, max, min + 1, max - 1}`;
///   * **interesting** — the curated `INTERESTING_INTEGERS` in range (zero, ±1,
///     the small magnitudes, powers of two and their neighbours, type limits);
///   * **diffuse** — the large `GLOBAL_CONSTANTS_INTEGERS` pool in range;
///   * **middle** — the ordinary piecewise distribution.
///
/// Splitting the range edges into their own category is what makes `x + y`
/// overflow (and other endpoint interactions) reachable: an endpoint-heavy case
/// draws both operands from `{min, max, …}`, so their sum overflows about half
/// the time. Drawing the weights per case, rather than fixing them, keeps most
/// cases middle-dominated ("normal") while a thin lumpy tail concentrates on one
/// special category — the correlation a fixed per-value probability can't
/// produce. Mirrors Hypothesis's boundary/`nasty` value injection (hypothesis#4722,
/// hegel-rust#350) crossed with swarm testing (Groce et al., ISSTA 2012).
///
/// These are pure reweightings: every category keeps positive probability under
/// every draw (the Dirichlet never assigns exactly zero), so which values are
/// reachable is unchanged — only how often.
///
/// The Dirichlet concentration parameters. Each is well below the middle's, so
/// the special categories are usually near zero and occasionally spike (the
/// lumpiness); their ratios set the mean weight of each category.
pub const DIRICHLET_ALPHA_ENDPOINT: f64 = 0.08;
pub const DIRICHLET_ALPHA_INTERESTING: f64 = 0.8;
pub const DIRICHLET_ALPHA_DIFFUSE: f64 = 0.12;
pub const DIRICHLET_ALPHA_MIDDLE: f64 = 2.2;

/// Minimum range width (`max_value - min_value`) at which the category mixture
/// is applied. Below this the ordinary piecewise distribution (uniform on
/// `[-256, 256]` at its core) already surfaces endpoints, zero and small
/// magnitudes frequently, so narrow draws keep their previous generation and
/// shrink behaviour byte-for-byte. The under-sampling only bites for *large or
/// full-width* ranges.
pub const CURATED_MIN_WIDTH: u128 = 256;

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
