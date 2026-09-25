use crate::native::HashMap;
use alloc::vec::Vec;

use crate::native::bignum::{BigInt, Sign, ToPrimitive};
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{
    ChoiceData, ChoiceNode, ChoiceValue, FloatChoice, float_to_index, index_to_float, sort_key,
};

use super::search::{BinSearchDown, FindInteger};
use super::{PassExit, ShrinkResult, ShrinkRun, Shrinker, absorb_node_gone};
use crate::control::{InternalError, hegel_internal_debug_assert};

/// Largest `f64` for which `n + 1.0 != n` holds — i.e., `2^53`. Above
/// this magnitude consecutive integers stop being individually
/// representable as `f64`, so any "redistribute" that bumps a float by
/// 1 silently reads as a shrink without actually changing the value.
const MAX_PRECISE_INTEGER: f64 = (1u64 << 53) as f64;

/// Decompose a positive finite float into `(m, n)` with `value == m / n`.
///
/// Returns `None` for values whose numerator or denominator doesn't fit
/// in `u128`: subnormals (denominator `2^1074`) and huge normals
/// (numerator > `2^127`) both overflow. Callers skip the integer-ratio
/// shrink step for those.
pub(super) fn as_integer_ratio(v: f64) -> Result<Option<(u128, u128)>, InternalError> {
    hegel_internal_debug_assert!(v.is_finite() && v > 0.0);
    let bits = v.to_bits();
    let biased_exp = ((bits >> 52) & 0x7FF) as i32;
    let mantissa_bits = bits & ((1u64 << 52) - 1);
    let (mut num, mut exp) = if biased_exp == 0 {
        (u128::from(mantissa_bits), -1074i32)
    } else {
        (
            u128::from((1u64 << 52) | mantissa_bits),
            biased_exp - 1023 - 52,
        )
    };
    let trailing = num.trailing_zeros() as i32;
    num >>= trailing;
    exp += trailing;
    Ok(if exp >= 0 {
        num.checked_shl(exp as u32).map(|shifted| (shifted, 1))
    } else {
        1u128.checked_shl((-exp) as u32).map(|n| (num, n))
    })
}

impl<'a> Shrinker<'a> {
    /// Shrink float choices toward simpler values using the float lex ordering.
    ///
    /// Steps per float node:
    /// 1. Try replacing with simplest(), then with the simplest non-zero
    ///    value. The second is the answer for a predicate that rejects zero
    ///    inside a bounded range, where the index bisection of step 4 is not
    ///    monotone: below the simplest in-range value every index decodes
    ///    out of range, and above it in- and out-of-range values interleave.
    /// 2. From ±inf, try ±f64::MAX (and -inf → +inf). Needed because the
    ///    later integer search saturates well below f64::MAX (i128::MAX as
    ///    f64 ≪ f64::MAX) and the lex-index bisection never lands on MAX's
    ///    all-ones mantissa.
    /// 3. If sign-negative, try negating (positive is simpler).
    /// 4. Binary search on absolute-value lex index from 0 toward current value.
    ///    Searching from 0 ensures we can find "nice" integer floats (like 2.0)
    ///    even when they have smaller lex indices than the boundary values.
    /// 5. Integer-ratio reduction: decompose v = k + r/n and shrink k toward
    ///    zero while holding the fractional remainder r/n fixed. Catches
    ///    shrinks like 2.5 → 1.5 under predicates that constrain the
    ///    fractional part.
    pub(super) async fn shrink_floats(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            absorb_node_gone(self.shrink_float_node(i).await)?;
            i += 1;
        }
        Ok(())
    }

    async fn shrink_float_node(&mut self, i: usize) -> Result<(), PassExit> {
        {
            if let ChoiceData::Float(fc, v) = &self.current_nodes[i].data {
                let v = *v;
                let fc = fc.clone();

                let s = fc.simplest()?;
                if ChoiceValue::Float(s) != ChoiceValue::Float(v) {
                    self.replace(&HashMap::from_iter([(i, ChoiceValue::Float(s))]))
                        .await?;
                }

                let v = self.float_at(i).ok_or(PassExit::NodeGone)?;
                if v != 0.0 {
                    let s = fc.simplest_nonzero()?;
                    if ChoiceValue::Float(s) != ChoiceValue::Float(v) {
                        self.replace(&HashMap::from_iter([(i, ChoiceValue::Float(s))]))
                            .await?;
                    }
                }

                let v = self.float_at(i).ok_or(PassExit::NodeGone)?;

                if v.is_infinite() {
                    if v < 0.0 && fc.validate(f64::INFINITY) {
                        self.replace(&HashMap::from_iter([(
                            i,
                            ChoiceValue::Float(f64::INFINITY),
                        )]))
                        .await?;
                    }
                    let v = self.float_at(i).ok_or(PassExit::NodeGone)?;
                    if v.is_infinite() {
                        let cand = if v > 0.0 { f64::MAX } else { -f64::MAX };
                        if fc.validate(cand) {
                            self.replace(&HashMap::from_iter([(i, ChoiceValue::Float(cand))]))
                                .await?;
                        }
                    }
                }

                let v = self.float_at(i).ok_or(PassExit::NodeGone)?;

                if v.is_nan() {
                    let mut stepped = false;
                    for cand in [f64::MAX, f64::INFINITY] {
                        if fc.validate(cand)
                            && self
                                .replace(&HashMap::from_iter([(i, ChoiceValue::Float(cand))]))
                                .await?
                        {
                            stepped = true;
                            break;
                        }
                    }
                    if !stepped && v.to_bits() != f64::NAN.to_bits() && fc.validate(f64::NAN) {
                        let mut attempt: Vec<ChoiceNode> = self.current_nodes.clone();
                        attempt[i] = ChoiceNode::float(fc.clone(), f64::NAN, attempt[i].was_forced);
                        let (is_interesting, actual_nodes, actual_spans) =
                            self.run_test_fn(ShrinkRun::Full(attempt)).await?;
                        self.calls += 1;
                        if is_interesting
                            && sort_key(&actual_nodes) <= sort_key(&self.current_nodes)
                        {
                            self.current_nodes = actual_nodes;
                            self.current_spans = actual_spans;
                        }
                    }
                }

                let v = self.float_at(i).ok_or(PassExit::NodeGone)?;

                if v.is_nan() {
                    return Ok(());
                }

                if v.is_sign_negative() {
                    let neg = -v;
                    if fc.validate(neg) {
                        self.replace(&HashMap::from_iter([(i, ChoiceValue::Float(neg))]))
                            .await?;
                    }
                }

                let v = self.float_at(i).ok_or(PassExit::NodeGone)?;

                let v_abs = v.abs();
                let is_neg = v.is_sign_negative();
                if v_abs.is_finite() && v_abs > 0.0 && v_abs >= MAX_PRECISE_INTEGER {
                    let base: i128 = if v_abs >= (i128::MAX as f64) {
                        i128::MAX
                    } else {
                        v_abs as i128
                    };
                    let i_capture = i;
                    let fc_capture = fc.clone();
                    let mut search = FindInteger::new();
                    while let Some(k) = search.probe() {
                        let ok = if k >= 127 {
                            false
                        } else {
                            let shifted = base >> k;
                            let candidate_mag = shifted as f64;
                            let candidate = if is_neg {
                                -candidate_mag
                            } else {
                                candidate_mag
                            };
                            if !fc_capture.validate(candidate) {
                                false
                            } else {
                                self.replace(&HashMap::from_iter([(
                                    i_capture,
                                    ChoiceValue::Float(candidate),
                                )]))
                                .await?
                            }
                        };
                        search.record(ok);
                    }
                    let cur = self.float_at(i).ok_or(PassExit::NodeGone)?;
                    if cur.is_finite() {
                        let base_after = cur.abs() as i128;
                        let lo: i128 = if is_neg {
                            libm::ceil((-fc.max_value).max(0.0)) as i128
                        } else {
                            libm::ceil(fc.min_value.max(0.0)) as i128
                        };
                        for step in [2i128, 1] {
                            let i_capture = i;
                            let mut search = FindInteger::new();
                            while let Some(n) = search.probe() {
                                let attempt = base_after - step * (n as i128);
                                let ok = if attempt < lo {
                                    false
                                } else {
                                    let candidate_mag = attempt as f64;
                                    let candidate = if is_neg {
                                        -candidate_mag
                                    } else {
                                        candidate_mag
                                    };
                                    self.replace(&HashMap::from_iter([(
                                        i_capture,
                                        ChoiceValue::Float(candidate),
                                    )]))
                                    .await?
                                };
                                search.record(ok);
                            }
                        }
                    }
                } else if v_abs.is_finite() && v_abs > 0.0 {
                    let cur = self.float_at(i).ok_or(PassExit::NodeGone)?;
                    let cur_abs = cur.abs();
                    for p in (0..=10).rev() {
                        let scale = libm::exp2(f64::from(p));
                        let scaled = cur_abs * scale;
                        for rounded in [libm::floor(scaled), libm::ceil(scaled)] {
                            let candidate_mag = rounded / scale;
                            if !candidate_mag.is_finite()
                                || float_to_index(candidate_mag) >= float_to_index(cur_abs)
                            {
                                continue;
                            }
                            let candidate = if is_neg {
                                -candidate_mag
                            } else {
                                candidate_mag
                            };
                            if fc.validate(candidate) {
                                self.replace(&HashMap::from_iter([(
                                    i,
                                    ChoiceValue::Float(candidate),
                                )]))
                                .await?;
                            }
                        }
                    }
                }

                let v = self.float_at(i).ok_or(PassExit::NodeGone)?;
                let v_abs = v.abs();
                let current_idx = float_to_index(v_abs);
                let is_neg = v.is_sign_negative();
                if current_idx > 0 {
                    let mut search = BinSearchDown::new(0, current_idx as i128);
                    while let Some(idx) = search.probe() {
                        let candidate_mag = index_to_float(idx as u64);
                        let candidate = if is_neg {
                            -candidate_mag
                        } else {
                            candidate_mag
                        };
                        let ok = if fc.validate(candidate) {
                            self.replace(&HashMap::from_iter([(i, ChoiceValue::Float(candidate))]))
                                .await?
                        } else {
                            false
                        };
                        search.record(ok);
                    }
                }

                let v = self.float_at(i).ok_or(PassExit::NodeGone)?;
                if v.is_finite() && v != 0.0 {
                    let is_neg = v.is_sign_negative();
                    if let Some((m, n)) = as_integer_ratio(v.abs())? {
                        let k_init = m / n;
                        let r = m % n;
                        if k_init > 0 {
                            let mut search = BinSearchDown::new(0, k_init as i128);
                            while let Some(k) = search.probe() {
                                let num_sum = (k as u128) * n + r;
                                let candidate_abs = (num_sum as f64) / (n as f64);
                                let candidate = if is_neg {
                                    -candidate_abs
                                } else {
                                    candidate_abs
                                };
                                let ok = if !fc.validate(candidate) {
                                    false
                                } else {
                                    let epoch = self.improvements;
                                    self.replace(&HashMap::from_iter([(
                                        i,
                                        ChoiceValue::Float(candidate),
                                    )]))
                                    .await?;
                                    self.improvements > epoch
                                };
                                search.record(ok);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Current float value at node `i`, or `None` when the node is not (or
    /// no longer) a float — a concurrent shrink can pun the kind at any
    /// position between probes.
    fn float_at(&self, i: usize) -> Option<f64> {
        let (_, v) = self.current_nodes.get(i)?.data.as_float()?;
        Some(v)
    }

    /// Redistribute magnitude across nearby numeric pairs.
    ///
    /// For sum-style constraints (`a + b > 1000`), shrinking `a` toward 0
    /// alone breaks the predicate; the pair only collapses to its minimum
    /// when `a` is reduced and `b` is raised by the same amount in
    /// lockstep. Walks pairs `(i, j)` where `j - i` is small (cap 4 to
    /// avoid quadratic scans), at least one side is a non-trivial Float,
    /// and probes `(v_i - k, v_j + k)` (or `(v_i + k, v_j - k)` if `v_i`
    /// is below its shrink target). Maximises `k` via `find_integer`.
    ///
    /// Pure Integer-Integer pairs are already handled by
    /// [`Shrinker::redistribute_integers`] — this pass complements it by
    /// covering Float-Float, Float-Integer, and Integer-Float pairs that
    /// the integer-only pass skips.
    pub(super) async fn redistribute_numeric_pairs(&mut self) -> ShrinkResult<()> {
        let len = self.current_nodes.len();
        for i in 0..len {
            for gap in 1..=4 {
                if i + gap >= self.current_nodes.len() {
                    break;
                }
                let j = i + gap;
                let (Some(num_i), Some(num_j)) = (
                    numeric_at(&self.current_nodes[i].data),
                    numeric_at(&self.current_nodes[j].data),
                ) else {
                    continue;
                };
                if matches!(
                    (&num_i, &num_j),
                    (Numeric::Integer(..), Numeric::Integer(..))
                ) {
                    continue;
                }
                if !num_i.can_choose_for_redistribute() || !num_j.can_choose_for_redistribute() {
                    continue;
                }
                if num_i.is_trivial()? {
                    continue;
                }
                redistribute_pair(self, i, num_i, j, num_j).await?;
            }
        }
        Ok(())
    }

    /// Trade magnitude between nearby numeric pairs along their product.
    ///
    /// [`Shrinker::redistribute_integers`] and
    /// [`Shrinker::redistribute_numeric_pairs`] keep a pair's *sum* fixed,
    /// the right move under `a + b > c`. Under a bound on a product —
    /// `a · k > MAX` with `k` in a small range, or `|to − from| > f64::MAX`
    /// with `to` a few ulps short of the far end of its range — lowering
    /// the earlier draw needs the later one raised by a proportional
    /// amount that the sum moves' unit steps never reach. For each pair
    /// `(i, j)` of two integers or two floats with `j - i <= 4`, both away
    /// from their shrink targets, this tries the later draw at the far end
    /// of its range and at twice and ten times its distance from its
    /// target, with the earlier draw's distance scaled down by the same
    /// factor so the product of the two distances is kept (integers try
    /// both roundings), and, for an earlier draw on the wrong side of its
    /// target, both draws mirrored around their targets. Every attempt
    /// lowers the earlier draw, so the sort key improves whatever happens
    /// to the later one, and the single-node passes then finish the
    /// earlier draw against the raised later one. Repeats on a pair while
    /// an attempt is accepted.
    pub(super) async fn scale_numeric_pairs(&mut self) -> ShrinkResult<()> {
        let len = self.current_nodes.len();
        for i in 0..len {
            for gap in 1..=4 {
                let j = i + gap;
                if j >= self.current_nodes.len() {
                    break;
                }
                loop {
                    let (Some(num_i), Some(num_j)) = (
                        self.current_nodes.get(i).and_then(|n| numeric_at(&n.data)),
                        self.current_nodes.get(j).and_then(|n| numeric_at(&n.data)),
                    ) else {
                        break;
                    };
                    let mut accepted = false;
                    for (val_i, val_j) in scaled_candidates(&num_i, &num_j) {
                        if self
                            .replace(&HashMap::from_iter([(i, val_i), (j, val_j)]))
                            .await?
                        {
                            accepted = true;
                            break;
                        }
                    }
                    if !accepted {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

/// The factors by which [`Shrinker::scale_numeric_pairs`] raises the later
/// draw's distance from its target, after trying the far end of its range.
const SCALE_FACTORS: [u32; 2] = [2, 10];

/// The `(earlier, later)` replacements [`Shrinker::scale_numeric_pairs`]
/// tries for a pair, in order: the mirror of a pair whose earlier draw is
/// below its target, then the later draw at its far bound and at each of
/// [`SCALE_FACTORS`] times its distance, with the earlier draw scaled down
/// to keep the product of the distances. Empty for a mixed integer/float
/// pair, or when either draw sits at its target.
fn scaled_candidates(num_i: &Numeric, num_j: &Numeric) -> Vec<(ChoiceValue, ChoiceValue)> {
    match (num_i, num_j) {
        (Numeric::Integer(ic_i, v_i), Numeric::Integer(ic_j, v_j)) => {
            scaled_integer_candidates(ic_i, v_i, ic_j, v_j)
        }
        (Numeric::Float(fc_i, v_i), Numeric::Float(fc_j, v_j)) => {
            scaled_float_candidates(fc_i, *v_i, fc_j, *v_j)
        }
        _ => Vec::new(),
    }
}

fn scaled_integer_candidates(
    ic_i: &IntegerChoice,
    v_i: &BigInt,
    ic_j: &IntegerChoice,
    v_j: &BigInt,
) -> Vec<(ChoiceValue, ChoiceValue)> {
    let (t_i, t_j) = (ic_i.clamped_shrink_towards(), ic_j.clamped_shrink_towards());
    let (d_i, d_j) = (v_i - &t_i, v_j - &t_j);
    if d_i.sign() == Sign::NoSign || d_j.sign() == Sign::NoSign {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut push = |a: BigInt, b: BigInt| {
        if ic_i.validate(&a) && ic_j.validate(&b) {
            out.push((ChoiceValue::Integer(a), ChoiceValue::Integer(b)));
        }
    };
    if d_i.sign() == Sign::Minus {
        push(&t_i - &d_i, &t_j - &d_j);
    }
    let bound = if d_j.sign() == Sign::Plus {
        &ic_j.max_value - &t_j
    } else {
        &ic_j.min_value - &t_j
    };
    let scaled: Vec<BigInt> = SCALE_FACTORS
        .iter()
        .map(|&f| &d_j * BigInt::from(f))
        .collect();
    let step_i = BigInt::from(if d_i.sign() == Sign::Plus { 1 } else { -1 });
    for raised in core::iter::once(bound).chain(scaled) {
        if raised.magnitude() <= d_j.magnitude() {
            continue;
        }
        let quotient_magnitude = (d_i.magnitude() * d_j.magnitude()) / raised.magnitude();
        let quotient = if d_i.sign() == Sign::Minus {
            -BigInt::from(quotient_magnitude)
        } else {
            BigInt::from(quotient_magnitude)
        };
        for scaled_i in [quotient.clone(), &quotient + &step_i] {
            if scaled_i.magnitude() < d_i.magnitude() {
                push(&t_i + &scaled_i, &t_j + &raised);
            }
        }
    }
    out
}

fn scaled_float_candidates(
    fc_i: &FloatChoice,
    v_i: f64,
    fc_j: &FloatChoice,
    v_j: f64,
) -> Vec<(ChoiceValue, ChoiceValue)> {
    if !v_i.is_finite() || !v_j.is_finite() || v_i == 0.0 || v_j == 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut push = |a: f64, b: f64| {
        if fc_i.validate(a) && fc_j.validate(b) {
            out.push((ChoiceValue::Float(a), ChoiceValue::Float(b)));
        }
    };
    if v_i.is_sign_negative() {
        push(-v_i, -v_j);
    }
    let bound = if v_j > 0.0 {
        fc_j.max_value.min(f64::MAX)
    } else {
        fc_j.min_value.max(-f64::MAX)
    };
    let scaled = SCALE_FACTORS.iter().map(|&f| v_j * f64::from(f));
    for raised in core::iter::once(bound).chain(scaled) {
        if !raised.is_finite() || raised.abs() <= v_j.abs() {
            continue;
        }
        let scaled_i = v_i * (v_j / raised);
        if scaled_i.abs() < v_i.abs() {
            push(scaled_i, raised);
        }
    }
    out
}

/// A numeric (integer or float) constraint/value pair, extracted from a
/// [`ChoiceData`] by [`numeric_at`]. Pairing the two keeps the arithmetic
/// helpers below total: a delta application preserves the variant, so
/// rebuilding a [`ChoiceValue`] can only fail validation, never mismatch.
#[derive(Clone)]
enum Numeric {
    Integer(IntegerChoice, BigInt),
    Float(FloatChoice, f64),
}

/// The numeric pair at `data`, when it is an integer or float node.
fn numeric_at(data: &ChoiceData) -> Option<Numeric> {
    match data {
        ChoiceData::Integer(ic, v) => Some(Numeric::Integer((**ic).clone(), v.clone())),
        ChoiceData::Float(fc, v) => Some(Numeric::Float(fc.clone(), *v)),
        _ => None,
    }
}

impl Numeric {
    fn as_f64(&self) -> f64 {
        match self {
            Numeric::Integer(_, n) => bigint_as_f64(n),
            Numeric::Float(_, f) => *f,
        }
    }

    /// Float `shrink_towards` is fixed at 0 and we don't carry it in
    /// [`FloatChoice`]; integers aim at their clamped `shrink_towards`.
    fn shrink_target(&self) -> f64 {
        match self {
            Numeric::Integer(ic, _) => bigint_as_f64(&ic.simplest()),
            Numeric::Float(..) => 0.0,
        }
    }

    /// The only node-level filter `redistribute_numeric_pairs` needs is the
    /// MAX_PRECISE_INTEGER / NaN / inf check for floats.
    fn can_choose_for_redistribute(&self) -> bool {
        match self {
            Numeric::Float(_, f) => f.is_finite() && f.abs() < MAX_PRECISE_INTEGER,
            Numeric::Integer(..) => true,
        }
    }

    fn is_trivial(&self) -> Result<bool, InternalError> {
        Ok(match self {
            Numeric::Integer(ic, v) => *v == ic.simplest(),
            Numeric::Float(fc, v) => !v.is_finite() || *v == fc.simplest()?,
        })
    }

    /// This pair with `k` added to its value; the constraint (and hence the
    /// variant) is preserved.
    fn add(&self, k: i128) -> Numeric {
        match self {
            Numeric::Integer(ic, n) => Numeric::Integer(ic.clone(), n + BigInt::from(k)),
            Numeric::Float(fc, f) => Numeric::Float(fc.clone(), *f + k as f64),
        }
    }

    /// The value as a [`ChoiceValue`], when it passes its own constraint's
    /// validation.
    fn build_value(&self) -> Option<ChoiceValue> {
        match self {
            Numeric::Integer(ic, n) => ic.value_from_bigint(n).map(ChoiceValue::Integer),
            Numeric::Float(fc, f) => fc.validate(*f).then_some(ChoiceValue::Float(*f)),
        }
    }
}

/// f64 of a [`BigInt`] for the redistribute direction heuristic; out-of-range
/// magnitudes saturate to infinity, which the sort-key check then rejects.
fn bigint_as_f64(n: &BigInt) -> f64 {
    n.to_f64().unwrap_or(f64::INFINITY)
}

/// Direction the integer-pair search moves `node[i]` in.
///
/// `v_i` is reduced toward its shrink target (0 for floats, simplest() for
/// integers); the matching adjustment to `v_j` raises it. If `v_i` is
/// already below its shrink target, both deltas flip sign.
async fn redistribute_pair(
    shrinker: &mut Shrinker<'_>,
    i: usize,
    num_i: Numeric,
    j: usize,
    num_j: Numeric,
) -> ShrinkResult<()> {
    let target_i = num_i.shrink_target();
    let dir = if num_i.as_f64() >= target_i {
        Direction::LowerLeftRaiseRight
    } else {
        Direction::RaiseLeftLowerRight
    };

    let mut search = FindInteger::new();
    while let Some(k) = search.probe() {
        let (cand_i, cand_j) = apply_delta(&num_i, &num_j, k as i128, dir);
        let ok = match cand_i.build_value() {
            None => false,
            Some(val_i) => match cand_j.build_value() {
                None => false,
                Some(val_j) => {
                    shrinker
                        .replace(&HashMap::from_iter([(i, val_i), (j, val_j)]))
                        .await?
                }
            },
        };
        search.record(ok);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Direction {
    /// v_i above shrink target: subtract k from v_i, add k to v_j.
    LowerLeftRaiseRight,
    /// v_i below shrink target: add k to v_i, subtract k from v_j.
    RaiseLeftLowerRight,
}

fn apply_delta(v_i: &Numeric, v_j: &Numeric, k: i128, dir: Direction) -> (Numeric, Numeric) {
    let signed_k_i = match dir {
        Direction::LowerLeftRaiseRight => -k,
        Direction::RaiseLeftLowerRight => k,
    };
    let signed_k_j = -signed_k_i;
    (v_i.add(signed_k_i), v_j.add(signed_k_j))
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_floats_tests.rs"]
mod tests;
