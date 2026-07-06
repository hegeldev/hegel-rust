//! Typed draw parameter handling for the `hegel_generate_*` C ABI.

pub mod internet;
pub mod regex;
pub mod special;
pub mod text;

use crate::control::hegel_internal_assert;
use crate::native::core::{EngineError, ManyState, NativeTestCase, Status};
use crate::native::intervalsets::IntervalSet;

pub use text::TextAlphabet;

/// Span labels for the engine-side compound draws, matching the
/// `hegel_label_t` values exported by the C ABI. Emitted internally so the
/// shrinker sees each compound string / structured draw as a unit.
pub(crate) const LABEL_REGEX: u64 = 17;
pub(crate) const LABEL_EMAIL: u64 = 18;
pub(crate) const LABEL_URL: u64 = 19;
pub(crate) const LABEL_DOMAIN: u64 = 20;
pub(crate) const LABEL_DATE: u64 = 21;
pub(crate) const LABEL_TIME: u64 = 22;
pub(crate) const LABEL_DATETIME: u64 = 23;
pub(crate) const LABEL_UUID: u64 = 24;
pub(crate) const LABEL_IP_ADDRESS: u64 = 25;
pub(crate) const LABEL_INTEGER: u64 = 26;
pub(crate) const LABEL_FLOAT: u64 = 27;
pub(crate) const LABEL_BOOLEAN: u64 = 28;
pub(crate) const LABEL_BYTES: u64 = 29;
pub(crate) const LABEL_STRING: u64 = 30;

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
    let v = ntc.draw_float(
        min_value,
        max_value,
        spec.allow_nan,
        spec.allow_infinity,
        snm,
    )?;
    Ok(if spec.width == 32 && v.is_finite() {
        f64::from(v as f32)
    } else {
        v
    })
}

/// A validated string-draw specification, the payload of a
/// `hegel_string_generator_t` handle. Built once via the smart constructors
/// (which report invalid parameters immediately), then drawn from any number
/// of times with [`generate_string`].
pub enum StringSpec {
    Text {
        intervals: IntervalSet,
        min_size: usize,
        max_size: usize,
    },
    Regex {
        compiled: regex::CompiledRegex,
        fullmatch: bool,
    },
    Email,
    Url,
    Domain {
        max_length: usize,
    },
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
            intervals,
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
            Some(StringSpec::Text { intervals, .. }) => Some(intervals.clone()),
            Some(_) => {
                return Err(EngineError::InvalidArgument(
                    "regex alphabet must be a text string generator".to_string(),
                ));
            }
        };
        Ok(StringSpec::Regex {
            compiled: regex::CompiledRegex::compile(pattern, alphabet)?,
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
    /// Errors when `max_length` leaves no eligible TLDs.
    pub fn domain(max_length: usize) -> Result<StringSpec, EngineError> {
        internet::validate_domain_max_length(max_length)?;
        Ok(StringSpec::Domain { max_length })
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
            ntc.draw_string(intervals.clone(), *min_size, *max_size)
        }),
        StringSpec::Regex {
            compiled,
            fullmatch,
        } => spanned(ntc, LABEL_REGEX, |ntc| {
            regex::generate_regex(ntc, compiled, *fullmatch)
        }),
        StringSpec::Email => spanned(ntc, LABEL_EMAIL, internet::generate_email),
        StringSpec::Url => spanned(ntc, LABEL_URL, internet::generate_url),
        StringSpec::Domain { max_length } => spanned(ntc, LABEL_DOMAIN, |ntc| {
            internet::generate_domain(ntc, *max_length)
        }),
    }
}

/// Run `f` inside a `label`ed span. The span is closed whether or not `f`
/// succeeds (closing after a freeze is a no-op — `freeze` already closed
/// every open span).
///
/// Every draw exposed at the API surface — the primitives included — is
/// wrapped in one of these, mirroring how the old schema interpreter wrapped
/// every schema dispatch: the shrinker's span-mutation machinery duplicates
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
    if state.rejections > std::cmp::max(3, 2 * state.count) {
        if state.count < state.min_size {
            ntc.conclude(Status::Invalid, None);
            return Err(EngineError::InvalidTestCase);
        } else {
            state.force_stop = true;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/draws/mod_tests.rs"]
mod tests;
