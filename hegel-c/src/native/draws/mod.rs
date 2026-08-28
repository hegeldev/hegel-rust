//! Typed draw parameter handling for the `hegel_generate_*` C ABI.

pub mod internet;
pub mod regex;
pub mod special;
pub mod text;

use crate::control::hegel_internal_assert;
use crate::native::bignum::BigInt;
use crate::native::core::{
    EngineError, FloatChoice, ManyState, NativeTestCase, RecursionState, Status, float_clamp,
};
use crate::native::intervalsets::IntervalSet;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;

pub use text::TextAlphabet;

use crate::hegel_label_t;

/// Span labels for the engine-side compound draws, derived from the
/// `hegel_label_t` values exported by the C ABI. Emitted internally so the
/// shrinker sees each compound string / structured draw as a unit.
pub(crate) const LABEL_REGEX: u64 = hegel_label_t::HEGEL_LABEL_REGEX as u64;
pub(crate) const LABEL_EMAIL: u64 = hegel_label_t::HEGEL_LABEL_EMAIL as u64;
pub(crate) const LABEL_URL: u64 = hegel_label_t::HEGEL_LABEL_URL as u64;
pub(crate) const LABEL_DOMAIN: u64 = hegel_label_t::HEGEL_LABEL_DOMAIN as u64;
pub(crate) const LABEL_DATE: u64 = hegel_label_t::HEGEL_LABEL_DATE as u64;
pub(crate) const LABEL_TIME: u64 = hegel_label_t::HEGEL_LABEL_TIME as u64;
pub(crate) const LABEL_DATETIME: u64 = hegel_label_t::HEGEL_LABEL_DATETIME as u64;
pub(crate) const LABEL_UUID: u64 = hegel_label_t::HEGEL_LABEL_UUID as u64;
pub(crate) const LABEL_IP_ADDRESS: u64 = hegel_label_t::HEGEL_LABEL_IP_ADDRESS as u64;
pub(crate) const LABEL_INTEGER: u64 = hegel_label_t::HEGEL_LABEL_INTEGER as u64;
pub(crate) const LABEL_FLOAT: u64 = hegel_label_t::HEGEL_LABEL_FLOAT as u64;
pub(crate) const LABEL_BOOLEAN: u64 = hegel_label_t::HEGEL_LABEL_BOOLEAN as u64;
pub(crate) const LABEL_BYTES: u64 = hegel_label_t::HEGEL_LABEL_BYTES as u64;
pub(crate) const LABEL_STRING: u64 = hegel_label_t::HEGEL_LABEL_STRING as u64;
pub(crate) const LABEL_FRESH_ID: u64 = hegel_label_t::HEGEL_LABEL_FRESH_ID as u64;
pub(crate) const LABEL_SET_CHOICE: u64 = hegel_label_t::HEGEL_LABEL_SET_CHOICE as u64;
pub(crate) const LABEL_CONCURRENCY: u64 = hegel_label_t::HEGEL_LABEL_CONCURRENCY as u64;

/// Parameters of a float draw as accepted at the `hegel_generate_float` API
/// surface. Width-32 handling (bound clamping, result rounding) and the
/// exclusive-bound adjustments happen inside [`generate_float`], so callers
/// pass their request verbatim.
pub struct FloatSpec {
    pub width: u32,
    pub min_value: f64,
    pub max_value: f64,
    pub allow_nan: bool,
    pub allow_infinity: bool,
    pub exclude_min: bool,
    pub exclude_max: bool,
    pub smallest_nonzero_magnitude: f64,
}

/// Draw a float according to `spec`, validating the spec first.
///
/// Mirrors Hypothesis's float strategy handling: width-32 bounds must be
/// exactly representable as `f32`, exclusive bounds step to the next
/// representable value at the requested width, width-32 draws clamp
/// infinite bounds into the `f32` range when infinities are disallowed,
/// and a finite width-32 result is rounded through `f32`.
pub fn generate_float(ntc: &mut NativeTestCase, spec: &FloatSpec) -> Result<f64, EngineError> {
    if spec.width != 32 && spec.width != 64 {
        return Err(EngineError::InvalidArgument(format!(
            "unsupported float width: {} — Hegel supports widths 32 and 64",
            spec.width
        )));
    }
    let snm = spec.smallest_nonzero_magnitude;
    if !(snm.is_finite() && snm > 0.0) {
        return Err(EngineError::InvalidArgument(format!(
            "smallest_nonzero_magnitude must be a positive finite float, got {snm}"
        )));
    }
    if spec.min_value.is_nan() || spec.max_value.is_nan() {
        return Err(EngineError::InvalidArgument(
            "float bounds must not be NaN".to_string(),
        ));
    }
    if spec.allow_nan && (spec.min_value.is_finite() || spec.max_value.is_finite()) {
        return Err(EngineError::InvalidArgument(
            "Cannot have allow_nan=true with min_value or max_value".to_string(),
        ));
    }
    if spec.allow_infinity && spec.min_value.is_finite() && spec.max_value.is_finite() {
        return Err(EngineError::InvalidArgument(
            "Cannot have allow_infinity=true with both min_value and max_value".to_string(),
        ));
    }
    if spec.width == 32 {
        for (name, bound) in [("min_value", spec.min_value), ("max_value", spec.max_value)] {
            if f64::from(bound as f32) != bound {
                return Err(EngineError::InvalidArgument(format!(
                    "{name} ({bound}) cannot be exactly represented as a float of width 32"
                )));
            }
        }
    }
    let mut min_value = spec.min_value;
    let mut max_value = spec.max_value;
    if spec.exclude_min {
        min_value = if spec.width == 32 {
            f64::from((min_value as f32).next_up())
        } else {
            min_value.next_up()
        };
    }
    if spec.exclude_max {
        max_value = if spec.width == 32 {
            f64::from((max_value as f32).next_down())
        } else {
            max_value.next_down()
        };
    }
    if spec.width == 32 && !spec.allow_infinity {
        min_value = min_value.max(f64::from(f32::MIN));
        max_value = max_value.min(f64::from(f32::MAX));
    }
    if min_value > max_value {
        return Err(EngineError::InvalidArgument(format!(
            "min_value ({min_value}) must be <= max_value ({max_value}) \
             after exclusive-bound adjustment"
        )));
    }
    let v = spanned(ntc, LABEL_FLOAT, |ntc| {
        ntc.draw_float(
            min_value,
            max_value,
            spec.allow_nan,
            spec.allow_infinity,
            snm,
        )
    })?;
    Ok(if spec.width == 32 && v.is_finite() {
        narrow_to_f32(min_value, max_value, snm, v)
    } else {
        v
    })
}

/// Round a finite width-32 draw through `f32`. A finite `f64` magnitude
/// beyond `f32::MAX` would round to infinity, so it is instead remapped
/// into the finite `f32` range (via the same mantissa-fraction clamp used
/// for out-of-range draws), keeping large *finite* `f32` values in the
/// distribution.
fn narrow_to_f32(min_value: f64, max_value: f64, snm: f64, v: f64) -> f64 {
    let narrowed = f64::from(v as f32);
    if narrowed.is_finite() {
        return narrowed;
    }
    let fc = FloatChoice {
        min_value: min_value.max(f64::from(f32::MIN)),
        max_value: max_value.min(f64::from(f32::MAX)),
        allow_nan: false,
        allow_infinity: false,
        smallest_nonzero_magnitude: snm,
    };
    f64::from(float_clamp(&fc, v) as f32)
}

/// Draw an integer in `[min_value, max_value]`, validating the bounds.
pub fn generate_integer(
    ntc: &mut NativeTestCase,
    min_value: &BigInt,
    max_value: &BigInt,
) -> Result<BigInt, EngineError> {
    if min_value > max_value {
        return Err(EngineError::InvalidArgument(format!(
            "generate_integer requires min_value <= max_value, \
             got [{min_value}, {max_value}]"
        )));
    }
    spanned(ntc, LABEL_INTEGER, |ntc| {
        ntc.draw_integer(min_value.clone(), max_value.clone())
    })
}

/// Draw an integer identifier unique within the test case's family,
/// recorded by value (see [`NativeTestCase::draw_fresh_id`]).
pub(crate) fn fresh_id(ntc: &mut NativeTestCase) -> Result<i64, EngineError> {
    spanned(ntc, LABEL_FRESH_ID, |ntc| ntc.draw_fresh_id())
}

/// Draw one of `members`, recorded by value rather than by index (see
/// [`NativeTestCase::draw_from_set`]).
pub(crate) fn choose_from(ntc: &mut NativeTestCase, members: &[i64]) -> Result<i64, EngineError> {
    spanned(ntc, LABEL_SET_CHOICE, |ntc| ntc.draw_from_set(members))
}

/// Draw a byte string with length in `[min_size, max_size]`, validating the
/// sizes.
pub fn generate_bytes(
    ntc: &mut NativeTestCase,
    min_size: usize,
    max_size: usize,
) -> Result<Vec<u8>, EngineError> {
    if min_size > max_size {
        return Err(EngineError::InvalidArgument(format!(
            "generate_bytes requires min_size <= max_size, \
             got [{min_size}, {max_size}]"
        )));
    }
    spanned(ntc, LABEL_BYTES, |ntc| ntc.draw_bytes(min_size, max_size))
}

/// Draw a weighted boolean, validating the probability and any forced value.
pub fn generate_boolean(
    ntc: &mut NativeTestCase,
    p: f64,
    forced: Option<bool>,
) -> Result<bool, EngineError> {
    if !(0.0..=1.0).contains(&p) {
        return Err(EngineError::InvalidArgument(format!(
            "generate_boolean(p = {p}) requires a probability in [0.0, 1.0]"
        )));
    }
    if forced == Some(true) && p == 0.0 {
        return Err(EngineError::InvalidArgument(
            "generate_boolean: cannot force true when p = 0.0".to_string(),
        ));
    }
    if forced == Some(false) && p == 1.0 {
        return Err(EngineError::InvalidArgument(
            "generate_boolean: cannot force false when p = 1.0".to_string(),
        ));
    }
    spanned(ntc, LABEL_BOOLEAN, |ntc| ntc.weighted_precise(p, forced))
}

/// A validated string-draw specification, the payload of a
/// `hegel_string_generator_t` handle. Built once via the smart constructors
/// (which report invalid parameters immediately), then drawn from any number
/// of times with [`generate_string`].
pub enum StringSpec {
    Text {
        intervals: Arc<IntervalSet>,
        min_size: usize,
        max_size: usize,
    },
    Regex {
        compiled: Box<regex::CompiledRegex>,
        fullmatch: bool,
    },
    Email,
    Url,
    Domain(internet::DomainSpec),
}

impl StringSpec {
    /// A text draw: strings of length `[min_size, max_size]` over the
    /// alphabet described by `alphabet`. Errors when `min_size > max_size`
    /// or the alphabet constraints leave no characters (unless
    /// `max_size == 0`).
    pub fn text(
        alphabet: &TextAlphabet,
        min_size: usize,
        max_size: usize,
    ) -> Result<StringSpec, EngineError> {
        if min_size > max_size {
            return Err(EngineError::InvalidArgument(format!(
                "text requires min_size <= max_size, got [{min_size}, {max_size}]"
            )));
        }
        let intervals = text::build_intervals(alphabet)?;
        if intervals.is_empty() && max_size > 0 {
            return Err(EngineError::InvalidArgument(
                "InvalidArgument: No valid characters in the specified range. \
                 The alphabet's codec/codepoint/category/include/exclude \
                 constraints leave no characters available."
                    .to_string(),
            ));
        }
        Ok(StringSpec::Text {
            intervals: Arc::new(intervals),
            min_size,
            max_size,
        })
    }

    /// A regex draw: strings matching `pattern`. `alphabet`, when given,
    /// must be a text spec; its intervals constrain the padding and
    /// wildcard characters. Errors on an invalid pattern.
    pub fn regex(
        pattern: &str,
        fullmatch: bool,
        alphabet: Option<&StringSpec>,
    ) -> Result<StringSpec, EngineError> {
        let alphabet = match alphabet {
            None => None,
            Some(StringSpec::Text { intervals, .. }) => Some((**intervals).clone()),
            Some(_) => {
                return Err(EngineError::InvalidArgument(
                    "regex alphabet must be a text string generator".to_string(),
                ));
            }
        };
        Ok(StringSpec::Regex {
            compiled: Box::new(regex::CompiledRegex::compile(pattern, alphabet)?),
            fullmatch,
        })
    }

    /// An RFC 5321/5322 email-address draw.
    pub fn email() -> StringSpec {
        StringSpec::Email
    }

    /// An RFC 3986 `http`/`https` URL draw.
    pub fn url() -> StringSpec {
        StringSpec::Url
    }

    /// An RFC 1035 domain-name draw with total length at most `max_length`.
    /// Errors when `max_length` is outside 4..=255.
    pub fn domain(max_length: usize) -> Result<StringSpec, EngineError> {
        Ok(StringSpec::Domain(internet::DomainSpec::new(max_length)?))
    }
}

/// Draw a string according to `spec`, wrapped in a span labeled by the
/// spec's kind so the shrinker treats each drawn string as a unit.
pub fn generate_string(ntc: &mut NativeTestCase, spec: &StringSpec) -> Result<String, EngineError> {
    match spec {
        StringSpec::Text {
            intervals,
            min_size,
            max_size,
        } => spanned(ntc, LABEL_STRING, |ntc| {
            ntc.draw_string(Arc::clone(intervals), *min_size, *max_size)
        }),
        StringSpec::Regex {
            compiled,
            fullmatch,
        } => spanned(ntc, LABEL_REGEX, |ntc| {
            regex::generate_regex(ntc, compiled, *fullmatch)
        }),
        StringSpec::Email => spanned(ntc, LABEL_EMAIL, internet::generate_email),
        StringSpec::Url => spanned(ntc, LABEL_URL, internet::generate_url),
        StringSpec::Domain(spec) => spanned(ntc, LABEL_DOMAIN, |ntc| {
            internet::generate_domain(ntc, spec)
        }),
    }
}

/// Run `f` inside a `label`ed span. The span is closed whether or not `f`
/// succeeds (closing after a freeze is a no-op — `freeze` already closed
/// every open span).
///
/// Every draw exposed at the API surface — the primitives included — is
/// wrapped in one of these: the shrinker's span-mutation machinery duplicates
/// same-label spans to propose values that already appear elsewhere in the
/// test case, which is how "find a list containing this integer"-shaped
/// examples are discovered.
pub(crate) fn spanned<R>(
    ntc: &mut NativeTestCase,
    label: u64,
    f: impl FnOnce(&mut NativeTestCase) -> Result<R, EngineError>,
) -> Result<R, EngineError> {
    ntc.start_span(label);
    let result = f(ntc);
    ntc.stop_span(false);
    result
}

/// Advance the many state by one element.  Returns true if another
/// element should be drawn.  Mirrors `Hypothesis`'s `many.more()`.
pub(crate) fn many_more(
    ntc: &mut NativeTestCase,
    state: &mut ManyState,
) -> Result<bool, EngineError> {
    let should_continue = if state.min_size as f64 == state.max_size {
        state.count < state.min_size
    } else {
        let forced = if state.force_stop {
            Some(false)
        } else if state.count < state.min_size {
            Some(true)
        } else if state.count as f64 >= state.max_size {
            Some(false)
        } else {
            None
        };
        ntc.weighted(state.p_continue, forced)?
    };

    if should_continue {
        state.count += 1;
    }
    Ok(should_continue)
}

/// Reject the last drawn element.  Mirrors Hypothesis's `many.reject()`.
pub(crate) fn many_reject(
    ntc: &mut NativeTestCase,
    state: &mut ManyState,
) -> Result<(), EngineError> {
    hegel_internal_assert!(state.count > 0);
    state.count -= 1;
    state.rejections += 1;
    if state.rejections > core::cmp::max(3, 2 * state.count) {
        if state.count < state.min_size {
            ntc.conclude(Status::Invalid, None);
            return Err(EngineError::InvalidTestCase);
        } else {
            state.force_stop = true;
        }
    }
    Ok(())
}

/// The number of budget-exceeded retries one recursive draw gets before
/// the test case is rejected: attempt `k` prices branches as if each had
/// `k` more children than observed, so nine attempts push any branch
/// function down to a fitting probability.
pub(crate) const RECURSION_MAX_ATTEMPTS: u64 = 9;

/// The number of completed-but-mispriced attempts one recursive draw
/// discards (see [`recursion_finish`]) before accepting whatever the
/// current price produces. One retry fixes the common case (the first
/// attempt priced with no arity evidence); the second covers a first
/// attempt so small its evidence was still badly noisy.
pub(crate) const RECURSION_MAX_REPRICES: u64 = 2;

/// The mean branch arity assumed before any branches have been observed.
const RECURSION_PRIOR_ARITY: f64 = 2.0;

/// The weight of [`RECURSION_PRIOR_ARITY`] in the arity estimate, in
/// observed-branch units. Small, so a handful of closed branches dominate
/// the prior; a branch function that always has two children matches the
/// prior and keeps the estimate pinned at exactly 2 regardless.
const RECURSION_PRIOR_WEIGHT: f64 = 0.25;

/// Ceiling on the branch probability. The pricing formula asks for `p = 1`
/// as the mean arity approaches 1 (a chain can never reach a leaf budget
/// above 1), and this cap is what bounds chain-like values instead:
/// unary runs end within a few dozen nodes rather than always slamming
/// into `max_depth`.
const RECURSION_MAX_BRANCH_PROBABILITY: f64 = 0.95;

/// How far above an attempt's starting branch probability the repriced
/// probability must sit before [`recursion_finish`] discards the attempt
/// as mispriced. Large enough that estimate noise around a correct price
/// never triggers it, small enough that pricing a mean arity of 1.5 as 2
/// (probability 0.66 as 0.50) does.
const RECURSION_REPRICE_THRESHOLD: f64 = 0.05;

/// The branch probability at which a tree whose branches average
/// `mean_arity` children has *expected* leaf count `max_leaves`.
///
/// Model an attempt as a branching process where every node independently
/// branches with probability `p` or is a leaf. Only the mean number of
/// children per branch matters for the expectation; with `k = mean_arity`,
/// the expected leaf count is `E[L] = (1 - p) / (1 - kp)` (from the
/// standard total-progeny mean `E[N] = 1 / (1 - kp)`), and solving
/// `E[L] = max_leaves` gives
///
/// ```text
/// p = (max_leaves - 1) / (max_leaves * k - 1)
/// ```
///
/// This sits just below the critical probability `1/k`, so sizes spread
/// over the whole budget — the distribution is heavily skewed, with most
/// attempts far below the mean and a heavy tail reaching (and past) it —
/// while the mean stays pinned to the budget instead of diverging the way
/// it does at criticality. Attempts in the tail beyond `max_leaves` are
/// discarded and retried by the protocol.
///
/// Boundary handling. Budgets below 2 use 2: at a budget of 1 the solution
/// degenerates to `p = 0`, which would make the single leaf the only
/// generable value (and make a budget of 0, whose only valid values are
/// leafless, unsatisfiable), while the hard budget stays enforced by the
/// retry protocol regardless of `p`. Budgets above 2^32 use 2^32, keeping
/// `p` strictly subcritical where the exact solution would round to `1/k`
/// in floating point; the probabilities it would round away from differ by
/// under 1e-10, far below anything observable. Mean arities at or below 1
/// use 1, where the solution reaches `p = 1` (no arity that low can grow
/// to any budget); the result is capped at
/// [`RECURSION_MAX_BRANCH_PROBABILITY`], which is what keeps that case —
/// and every near-1 arity whose exact solution exceeds the cap — finite.
pub(crate) fn recursion_branch_probability(mean_arity: f64, max_leaves: u64) -> f64 {
    let k = mean_arity.max(1.0);
    let budget = max_leaves.clamp(2, 1 << 32) as f64;
    ((budget - 1.0) / (budget * k - 1.0)).min(RECURSION_MAX_BRANCH_PROBABILITY)
}

/// The mean branch arity suggested by the branches observed so far, blended
/// with [`RECURSION_PRIOR_ARITY`] at [`RECURSION_PRIOR_WEIGHT`]. Only
/// *closed* branches count: a branch still awaiting children would
/// understate its arity, and on the deep left spine of a wide tree that
/// understatement would briefly (and wrongly) price the grammar as
/// chain-like.
fn recursion_arity_estimate(state: &RecursionState) -> f64 {
    (RECURSION_PRIOR_ARITY * RECURSION_PRIOR_WEIGHT + state.closed_children as f64)
        / (RECURSION_PRIOR_WEIGHT + state.closed_branches as f64)
}

/// The branch probability priced from the draw's current arity evidence:
/// [`recursion_branch_probability`] at the estimated mean arity, plus one
/// assumed extra child per branch for each budget-exceeded retry so far so
/// that repeated overruns drive the probability down even when the arity
/// estimate has stopped moving. With no evidence beyond the prior this is
/// exactly the binary-tree price, and for a branch function whose branches
/// all have two children it never moves — every closed branch confirms the
/// prior.
fn recursion_priced_probability(state: &RecursionState) -> f64 {
    recursion_branch_probability(
        recursion_arity_estimate(state) + state.attempt as f64,
        state.max_leaves,
    )
}

/// Create the state for one recursive draw, with the first attempt's
/// branch probability priced for a binary tree (the prior: no branches
/// have been observed yet).
pub(crate) fn new_recursion_state(
    max_depth: u64,
    max_leaves: u64,
    base_span_depth: usize,
) -> RecursionState {
    let mut state = RecursionState {
        max_depth,
        max_leaves,
        attempt: 0,
        leaves: 0,
        base_span_depth,
        branch_probability: 0.0,
        closed_children: 0,
        closed_branches: 0,
        open_branches: Vec::new(),
        reprices: 0,
    };
    state.branch_probability = recursion_priced_probability(&state);
    state
}

/// Draw the leaf-or-branch decision for one sub-value of a recursive
/// generator.
///
/// The probability comes from [`recursion_priced_probability`]: the value
/// at which a tree of the arity observed so far has expected leaf count
/// equal to the leaf budget. It is recomputed at every decision as branches
/// close and sharpen the arity estimate — for a branch function whose
/// branches always have two children the estimate never moves, so the
/// probability stays constant across the attempt — and it deliberately
/// does *not* depend on the budget already spent, which would make earlier
/// subtrees systematically branchier than their later siblings. At the depth limit the decision is still drawn, at
/// probability zero: the engine records it as a forced choice, so the
/// choice sequence has the same shape whether or not the limit was hit.
pub(crate) fn recursion_branch(
    ntc: &mut NativeTestCase,
    state: &mut RecursionState,
    depth: u64,
) -> Result<bool, EngineError> {
    state.observe_decision(depth);
    let p = if depth >= state.max_depth {
        0.0
    } else {
        recursion_priced_probability(state)
    };
    let branch = spanned(ntc, LABEL_BOOLEAN, |ntc| ntc.weighted_precise(p, None))?;
    if branch {
        state.observe_branch(depth);
    }
    Ok(branch)
}

/// Discard a generation attempt that exceeded its leaf budget: close the
/// spans the attempt left open (marking them discarded), drop the
/// observations the unwind cut short, reset the budget, and move to the
/// next attempt's lower branch probability. Concludes the test case as
/// invalid once [`RECURSION_MAX_ATTEMPTS`] attempts have failed.
pub(crate) fn recursion_retry(
    ntc: &mut NativeTestCase,
    state: &mut RecursionState,
) -> Result<(), EngineError> {
    while ntc.span_depth() > state.base_span_depth {
        ntc.stop_span(true);
    }
    state.discard_open_branches();
    state.leaves = 0;
    state.attempt += 1;
    if state.attempt >= RECURSION_MAX_ATTEMPTS {
        ntc.conclude(Status::Invalid, None);
        return Err(EngineError::InvalidTestCase);
    }
    state.branch_probability = recursion_priced_probability(state);
    Ok(())
}

/// Decide whether a completed generation attempt was priced fairly, now
/// that the finished value's branch arities are known exactly. Returns
/// `Ok(true)` to accept the value, or `Ok(false)` when the attempt was
/// mispriced and has been discarded (its spans closed as discarded, like a
/// budget retry): the caller regenerates the whole value from the root.
///
/// The first attempt of every draw is priced from the prior alone, and for
/// a branch function averaging fewer than two children per branch that
/// price is too low: the branching process it induces is firmly
/// subcritical, so values collapse to a handful of nodes, never exceed the
/// leaf budget, and the budget-retry path never gets a chance to correct
/// anything. The completed value is itself the evidence: when the
/// probability repriced from its observed arities exceeds the price the
/// attempt started from by more than [`RECURSION_REPRICE_THRESHOLD`], the
/// attempt is discarded and regenerated at the corrected price, at most
/// [`RECURSION_MAX_REPRICES`] times per draw. A price that was too *high*
/// never triggers this — over-branching attempts correct themselves
/// through the budget-retry path. Values whose price was consistent with
/// their observed arity — every value of a fixed-binary branch function,
/// and any value with no branches at all — are always accepted, so
/// adaptation costs nothing where pricing was already right and never
/// biases against small values.
pub(crate) fn recursion_finish(
    ntc: &mut NativeTestCase,
    state: &mut RecursionState,
) -> Result<bool, EngineError> {
    state.close_remaining_branches();
    if state.reprices >= RECURSION_MAX_REPRICES {
        return Ok(true);
    }
    let repriced = recursion_priced_probability(state);
    if repriced - state.branch_probability <= RECURSION_REPRICE_THRESHOLD {
        return Ok(true);
    }
    while ntc.span_depth() > state.base_span_depth {
        ntc.stop_span(true);
    }
    state.leaves = 0;
    state.reprices += 1;
    state.branch_probability = repriced;
    Ok(false)
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/draws/mod_tests.rs"]
mod tests;
