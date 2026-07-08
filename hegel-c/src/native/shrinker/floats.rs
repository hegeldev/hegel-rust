use std::collections::HashMap;

use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::choices::IntegerChoice;
use crate::native::core::{
    ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, float_to_index, index_to_float, sort_key,
};

use super::{ShrinkResult, ShrinkRun, Shrinker, bin_search_down_r, find_integer_r};
use crate::control::hegel_internal_debug_assert;

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
pub(super) fn as_integer_ratio(v: f64) -> Option<(u128, u128)> {
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
    if exp >= 0 {
        let shifted = num.checked_shl(exp as u32)?;
        Some((shifted, 1))
    } else {
        let n = 1u128.checked_shl((-exp) as u32)?;
        Some((num, n))
    }
}

impl<'a> Shrinker<'a> {
    /// Shrink float choices toward simpler values using the float lex ordering.
    ///
    /// Steps per float node:
    /// 1. Try replacing with simplest().
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
    pub(super) fn shrink_floats(&mut self) -> ShrinkResult<()> {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Float(fc), ChoiceValue::Float(v)) =
                (node.kind.as_ref(), &node.value)
            {
                let v = *v;
                let fc = fc.clone();

                let s = fc.simplest();
                if ChoiceValue::Float(s) != ChoiceValue::Float(v) {
                    self.replace(&HashMap::from([(i, ChoiceValue::Float(s))]))?;
                }

                let v = self.float_at(i);

                if v.is_infinite() {
                    if v < 0.0 && fc.validate(f64::INFINITY) {
                        self.replace(&HashMap::from([(i, ChoiceValue::Float(f64::INFINITY))]))?;
                    }
                    let v = self.float_at(i);
                    if v.is_infinite() {
                        let cand = if v > 0.0 { f64::MAX } else { -f64::MAX };
                        if fc.validate(cand) {
                            self.replace(&HashMap::from([(i, ChoiceValue::Float(cand))]))?;
                        }
                    }
                }

                let v = self.float_at(i);

                if v.is_nan() {
                    let mut stepped = false;
                    for cand in [f64::MAX, f64::INFINITY] {
                        if fc.validate(cand)
                            && self.replace(&HashMap::from([(i, ChoiceValue::Float(cand))]))?
                        {
                            stepped = true;
                            break;
                        }
                    }
                    if !stepped && v.to_bits() != f64::NAN.to_bits() && fc.validate(f64::NAN) {
                        let mut attempt: Vec<ChoiceNode> = self.current_nodes.clone();
                        attempt[i] = attempt[i].with_value(ChoiceValue::Float(f64::NAN));
                        let (is_interesting, actual_nodes, actual_spans) =
                            self.run_test_fn(ShrinkRun::Full(&attempt))?;
                        self.calls += 1;
                        if is_interesting
                            && sort_key(&actual_nodes) <= sort_key(&self.current_nodes)
                        {
                            self.current_nodes = actual_nodes;
                            self.current_spans = actual_spans;
                        }
                    }
                }

                let v = self.float_at(i);

                if v.is_nan() {
                    i += 1;
                    continue;
                }

                if v.is_sign_negative() {
                    let neg = -v;
                    if fc.validate(neg) {
                        self.replace(&HashMap::from([(i, ChoiceValue::Float(neg))]))?;
                    }
                }

                let v = self.float_at(i);

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
                    find_integer_r(|k| {
                        if k >= 127 {
                            return Ok(false);
                        }
                        let shifted = base >> k;
                        let candidate_mag = shifted as f64;
                        let candidate = if is_neg {
                            -candidate_mag
                        } else {
                            candidate_mag
                        };
                        if !fc_capture.validate(candidate) {
                            return Ok(false);
                        }
                        self.replace(&HashMap::from([(i_capture, ChoiceValue::Float(candidate))]))
                    })?;
                    let cur = self.float_at(i);
                    if cur.is_finite() {
                        let base_after = cur.abs() as i128;
                        let lo: i128 = if is_neg {
                            (-fc.max_value).max(0.0).ceil() as i128
                        } else {
                            fc.min_value.max(0.0).ceil() as i128
                        };
                        for step in [2i128, 1] {
                            let i_capture = i;
                            find_integer_r(|n| {
                                let attempt = base_after - step * (n as i128);
                                if attempt < lo {
                                    return Ok(false);
                                }
                                let candidate_mag = attempt as f64;
                                let candidate = if is_neg {
                                    -candidate_mag
                                } else {
                                    candidate_mag
                                };
                                self.replace(&HashMap::from([(
                                    i_capture,
                                    ChoiceValue::Float(candidate),
                                )]))
                            })?;
                        }
                    }
                } else if v_abs.is_finite() && v_abs > 0.0 {
                    let cur_abs = self.float_at(i).abs();
                    for p in (0..=10).rev() {
                        let scale = (2_f64).powi(p);
                        let scaled = cur_abs * scale;
                        for rounded in [scaled.floor(), scaled.ceil()] {
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
                                self.replace(&HashMap::from([(i, ChoiceValue::Float(candidate))]))?;
                            }
                        }
                    }
                }

                let v = self.float_at(i);
                let v_abs = v.abs();
                let current_idx = float_to_index(v_abs);
                let is_neg = v.is_sign_negative();
                if current_idx > 0 {
                    bin_search_down_r(0, current_idx as i128, &mut |idx| {
                        let candidate_mag = index_to_float(idx as u64);
                        let candidate = if is_neg {
                            -candidate_mag
                        } else {
                            candidate_mag
                        };
                        if fc.validate(candidate) {
                            self.replace(&HashMap::from([(i, ChoiceValue::Float(candidate))]))
                        } else {
                            Ok(false)
                        }
                    })?;
                }

                let v = self.float_at(i);
                if v.is_finite() && v != 0.0 {
                    let is_neg = v.is_sign_negative();
                    if let Some((m, n)) = as_integer_ratio(v.abs()) {
                        let k_init = m / n;
                        let r = m % n;
                        if k_init > 0 {
                            bin_search_down_r(0, k_init as i128, &mut |k| {
                                let num_sum = (k as u128) * n + r;
                                let candidate_abs = (num_sum as f64) / (n as f64);
                                let candidate = if is_neg {
                                    -candidate_abs
                                } else {
                                    candidate_abs
                                };
                                if !fc.validate(candidate) {
                                    return Ok(false);
                                }
                                let epoch = self.improvements;
                                self.replace(&HashMap::from([(i, ChoiceValue::Float(candidate))]))?;
                                Ok(self.improvements > epoch)
                            })?;
                        }
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }

    fn float_at(&self, i: usize) -> f64 {
        match self.current_nodes[i].value {
            ChoiceValue::Float(f) => f,
            _ => unreachable!("kind/value invariant violated: outer match guaranteed this variant"),
        }
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
    pub(super) fn redistribute_numeric_pairs(&mut self) -> ShrinkResult<()> {
        let len = self.current_nodes.len();
        for i in 0..len {
            for gap in 1..=4 {
                if i + gap >= self.current_nodes.len() {
                    break;
                }
                let j = i + gap;
                if !is_float_or_integer(self.current_nodes[i].kind.as_ref())
                    || !is_float_or_integer(self.current_nodes[j].kind.as_ref())
                {
                    continue;
                }
                if matches!(
                    (
                        self.current_nodes[i].kind.as_ref(),
                        self.current_nodes[j].kind.as_ref()
                    ),
                    (ChoiceKind::Integer(_), ChoiceKind::Integer(_))
                ) {
                    continue;
                }
                if !can_choose_for_redistribute(&self.current_nodes[i])
                    || !can_choose_for_redistribute(&self.current_nodes[j])
                {
                    continue;
                }
                if is_trivial(&self.current_nodes[i]) {
                    continue;
                }
                redistribute_pair(self, i, j)?;
            }
        }
        Ok(())
    }
}

/// Float `shrink_towards` is fixed at 0 and we don't carry it in
/// [`FloatChoice`]; the only node-level filter
/// `redistribute_numeric_pairs` needs is the MAX_PRECISE_INTEGER / NaN
/// / inf check below.
fn can_choose_for_redistribute(node: &ChoiceNode) -> bool {
    match (node.kind.as_ref(), &node.value) {
        (ChoiceKind::Float(_), ChoiceValue::Float(f)) => {
            f.is_finite() && f.abs() < MAX_PRECISE_INTEGER
        }
        (ChoiceKind::Integer(_), ChoiceValue::Integer(_)) => true,
        _ => unreachable!("filtered by is_float_or_integer; ChoiceNode invariant otherwise"),
    }
}

fn is_float_or_integer(k: &ChoiceKind) -> bool {
    match k {
        ChoiceKind::Float(_) | ChoiceKind::Integer(_) => true,
        ChoiceKind::Boolean(_)
        | ChoiceKind::Bytes(_)
        | ChoiceKind::String(_)
        | ChoiceKind::Clone => false,
    }
}

fn is_trivial(node: &ChoiceNode) -> bool {
    match (node.kind.as_ref(), &node.value) {
        (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => *v == ic.simplest(),
        (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => !v.is_finite() || *v == fc.simplest(),
        _ => unreachable!("filtered by is_float_or_integer; ChoiceNode invariant otherwise"),
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
fn redistribute_pair(shrinker: &mut Shrinker<'_>, i: usize, j: usize) -> ShrinkResult<()> {
    let (v_i, kind_i) = match (
        shrinker.current_nodes[i].kind.as_ref(),
        &shrinker.current_nodes[i].value,
    ) {
        (ChoiceKind::Integer(ic), ChoiceValue::Integer(n)) => (
            NumericValue::Integer(n.clone()),
            NumericKind::Integer(ic.clone()),
        ),
        (ChoiceKind::Float(fc), ChoiceValue::Float(f)) => {
            (NumericValue::Float(*f), NumericKind::Float(fc.clone()))
        }
        _ => unreachable!("redistribute_pair gated on is_float_or_integer + is_trivial"),
    };
    let (v_j, kind_j) = match (
        shrinker.current_nodes[j].kind.as_ref(),
        &shrinker.current_nodes[j].value,
    ) {
        (ChoiceKind::Integer(ic), ChoiceValue::Integer(n)) => (
            NumericValue::Integer(n.clone()),
            NumericKind::Integer(ic.clone()),
        ),
        (ChoiceKind::Float(fc), ChoiceValue::Float(f)) => {
            (NumericValue::Float(*f), NumericKind::Float(fc.clone()))
        }
        _ => unreachable!("redistribute_pair gated on is_float_or_integer + is_trivial"),
    };

    let target_i = shrink_target(&kind_i);
    let dir = if v_i.as_f64() >= target_i {
        Direction::LowerLeftRaiseRight
    } else {
        Direction::RaiseLeftLowerRight
    };

    find_integer_r(|k| {
        let (cand_i, cand_j) = apply_delta(&v_i, &v_j, k as i128, dir);
        let Some(val_i) = build_value(&kind_i, cand_i) else {
            return Ok(false);
        };
        let Some(val_j) = build_value(&kind_j, cand_j) else {
            return Ok(false);
        };
        shrinker.replace(&HashMap::from([(i, val_i), (j, val_j)]))
    })?;
    Ok(())
}

#[derive(Clone, Copy)]
enum Direction {
    /// v_i above shrink target: subtract k from v_i, add k to v_j.
    LowerLeftRaiseRight,
    /// v_i below shrink target: add k to v_i, subtract k from v_j.
    RaiseLeftLowerRight,
}

#[derive(Clone)]
enum NumericValue {
    Integer(BigInt),
    Float(f64),
}

impl NumericValue {
    fn as_f64(&self) -> f64 {
        match self {
            NumericValue::Integer(n) => bigint_as_f64(n),
            NumericValue::Float(f) => *f,
        }
    }
}

#[derive(Clone)]
enum NumericKind {
    Integer(IntegerChoice),
    Float(FloatChoice),
}

fn shrink_target(kind: &NumericKind) -> f64 {
    match kind {
        NumericKind::Integer(ic) => bigint_as_f64(&ic.simplest()),
        NumericKind::Float(_) => 0.0,
    }
}

fn apply_delta(
    v_i: &NumericValue,
    v_j: &NumericValue,
    k: i128,
    dir: Direction,
) -> (NumericValue, NumericValue) {
    let signed_k_i = match dir {
        Direction::LowerLeftRaiseRight => -k,
        Direction::RaiseLeftLowerRight => k,
    };
    let signed_k_j = -signed_k_i;
    (add_int(v_i, signed_k_i), add_int(v_j, signed_k_j))
}

fn add_int(v: &NumericValue, k: i128) -> NumericValue {
    match v {
        NumericValue::Integer(n) => NumericValue::Integer(n + BigInt::from(k)),
        NumericValue::Float(f) => NumericValue::Float(*f + k as f64),
    }
}

fn build_value(kind: &NumericKind, candidate: NumericValue) -> Option<ChoiceValue> {
    match (kind, candidate) {
        (NumericKind::Integer(ic), NumericValue::Integer(n)) => ic
            .value_from_bigint(&n)
            .filter(|av| ic.validate(av))
            .map(ChoiceValue::Integer),
        (NumericKind::Float(fc), NumericValue::Float(f)) => {
            fc.validate(f).then_some(ChoiceValue::Float(f))
        }
        _ => unreachable!("apply_delta preserves variant; kind and value cannot disagree"),
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_floats_tests.rs"]
mod tests;
