// Float shrink passes: `shrink_floats` reduces individual float choices toward
// the simplest value under Hypothesis's lex ordering, and
// `redistribute_numeric_pairs` rebalances adjacent (float, float),
// (float, integer), and (integer, float) pairs so sum-style predicates like
// `a + b > 1000` can collapse to their joint minimum.

use std::collections::HashMap;

use crate::native::core::choices::IntegerChoice;
use crate::native::core::{
    ChoiceKind, ChoiceNode, ChoiceValue, FloatChoice, float_to_index, index_to_float, sort_key,
};

use super::{ShrinkRun, Shrinker, bin_search_down, find_integer};

/// Largest `f64` for which `n + 1.0 != n` holds — i.e., `2^53`.  Above
/// this magnitude consecutive integers stop being individually
/// representable as `f64`, so any "redistribute" that bumps a float by
/// 1 silently reads as a shrink without actually changing the value.
/// Mirrors `hypothesis.internal.floats.MAX_PRECISE_INTEGER`.
const MAX_PRECISE_INTEGER: f64 = (1u64 << 53) as f64;

/// Decompose a positive finite float into `(m, n)` with `value == m / n`.
///
/// Mirrors Python's `float.as_integer_ratio`. Returns `None` for values whose
/// numerator or denominator doesn't fit in `u128`: subnormals (denominator
/// `2^1074`) and huge normals (numerator > `2^127`) both overflow. Callers
/// skip the integer-ratio shrink step for those.
pub(super) fn as_integer_ratio(v: f64) -> Option<(u128, u128)> {
    debug_assert!(v.is_finite() && v > 0.0);
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
    /// Shrink float choices toward simpler values using Hypothesis lex ordering.
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
    pub(super) fn shrink_floats(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Float(fc), ChoiceValue::Float(v)) = (&node.kind, &node.value) {
                let v = *v;
                let fc = fc.clone();

                // Step 1: Try simplest.
                let s = fc.simplest();
                if ChoiceValue::Float(s) != ChoiceValue::Float(v) {
                    self.replace(&HashMap::from([(i, ChoiceValue::Float(s))]));
                }

                let v = self.float_at(i);

                // Step 2: Special-value transitions out of ±inf.
                if v.is_infinite() {
                    if v < 0.0 && fc.validate(f64::INFINITY) {
                        self.replace(&HashMap::from([(i, ChoiceValue::Float(f64::INFINITY))]));
                    }
                    let v = self.float_at(i);
                    if v.is_infinite() {
                        let cand = if v > 0.0 { f64::MAX } else { -f64::MAX };
                        if fc.validate(cand) {
                            self.replace(&HashMap::from([(i, ChoiceValue::Float(cand))]));
                        }
                    }
                }

                let v = self.float_at(i);

                // NaN canonicalization. Mirrors Python's
                // `Float.short_circuit`, which considers
                // `[sys.float_info.max, inf, nan]` when current is NaN so
                // that unconstrained predicates escape to a finite value and
                // `is_nan`-style predicates converge on the positive
                // canonical NaN (`0x7ff8_0000_0000_0000`, smallest mantissa
                // in lex order). The non-NaN candidates go through
                // `replace`/`consider` unchanged; the canonical-NaN
                // fallback has to bypass `consider`'s sort-key shortcut,
                // since all NaN bit patterns share
                // `sort_index = (u64::MAX, false)` and `consider` would
                // otherwise return true without rewriting `current_nodes[i]`.
                if v.is_nan() {
                    let mut stepped = false;
                    for cand in [f64::MAX, f64::INFINITY] {
                        if fc.validate(cand)
                            && self.replace(&HashMap::from([(i, ChoiceValue::Float(cand))]))
                        {
                            stepped = true;
                            break;
                        }
                    }
                    if !stepped && v.to_bits() != f64::NAN.to_bits() && fc.validate(f64::NAN) {
                        let mut attempt: Vec<ChoiceNode> = self.current_nodes.clone();
                        attempt[i] = attempt[i].with_value(ChoiceValue::Float(f64::NAN));
                        let (is_interesting, actual_nodes) =
                            (self.test_fn)(ShrinkRun::Full(&attempt));
                        // Accept as a lateral move: all NaN bit patterns
                        // share sort_key so `<` alone would reject, but
                        // guard against a (hypothetical) test that
                        // produces a strictly worse sequence.
                        if is_interesting
                            && sort_key(&actual_nodes) <= sort_key(&self.current_nodes)
                        {
                            self.current_nodes = actual_nodes;
                        }
                    }
                }

                let v = self.float_at(i);

                // Skip NaN — can't binary search on NaN.
                if v.is_nan() {
                    i += 1;
                    continue;
                }

                // Step 3: Try negating if sign-negative (positive is simpler).
                if v.is_sign_negative() {
                    let neg = -v;
                    if fc.validate(neg) {
                        self.replace(&HashMap::from([(i, ChoiceValue::Float(neg))]));
                    }
                }

                // After negation, v is still finite non-NaN: simplest/negation only
                // produce finite non-NaN candidates, and a failed replace leaves the
                // (finite non-NaN) value in place.
                let v = self.float_at(i);

                // Step 4a: When current is a non-integer, explicitly search the
                // integer-float range.  In our ordering, integer floats 0, 1, 2, …
                // have indices 0, 1, 2, … (much smaller than any non-integer).
                // The existing binary search below misses them because it jumps
                // past small indices when hi is near 2^63.
                let v_abs = v.abs();
                let current_idx = float_to_index(v_abs);
                let is_neg = v.is_sign_negative();
                if current_idx >= (1u64 << 63) {
                    // Compute the integer magnitude range valid for fc. The bounds
                    // below keep candidates strictly inside [fc.min_value,
                    // fc.max_value], so fc.validate is guaranteed to hold.
                    let (int_lo, int_hi) = if is_neg {
                        // Negative float: candidate = -(n as f64). v < 0 implies
                        // fc.min_value < 0, so the `hi` expression is well-defined.
                        let lo = if fc.max_value <= 0.0 {
                            (-fc.max_value).ceil() as i128
                        } else {
                            0
                        };
                        let hi = (-fc.min_value).floor() as i128;
                        (lo, hi)
                    } else {
                        let lo = fc.min_value.max(0.0).ceil() as i128;
                        let hi = fc.max_value.min(f64::MAX).floor() as i128;
                        (lo, hi)
                    };
                    if int_lo >= 0 && int_lo <= int_hi {
                        let i_capture = i;
                        let is_neg_capture = is_neg;
                        bin_search_down(int_lo, int_hi, &mut |n| {
                            let candidate = if is_neg_capture {
                                -(n as f64)
                            } else {
                                n as f64
                            };
                            self.replace(&HashMap::from([(
                                i_capture,
                                ChoiceValue::Float(candidate),
                            )]))
                        });
                    }
                }

                // Step 4b: Binary search on absolute-value lex index toward 0.
                // Integer replacement above only produces finite non-NaN values.
                let v = self.float_at(i);
                let v_abs = v.abs();
                let current_idx = float_to_index(v_abs);
                let is_neg = v.is_sign_negative();
                if current_idx > 0 {
                    bin_search_down(0, current_idx as i128, &mut |idx| {
                        let candidate_mag = index_to_float(idx as u64);
                        let candidate = if is_neg {
                            -candidate_mag
                        } else {
                            candidate_mag
                        };
                        if fc.validate(candidate) {
                            self.replace(&HashMap::from([(i, ChoiceValue::Float(candidate))]))
                        } else {
                            false
                        }
                    });
                }

                // Step 5: Integer-ratio numeric reduction.
                //
                // Port of Hypothesis `conjecture/shrinking/floats.py::Float.run_step`
                // tail: decompose v = m/n exactly and binary-search the integer
                // part k of `divmod(m, n)` toward zero, keeping the fractional
                // remainder r/n fixed. Catches shrinks like 2.5 → 1.5 under
                // `fract(x) == 0.5` where neither the integer-range search
                // (Step 4a) nor the lex-index bisection (Step 4b) visit 1.5:
                // integer candidates have fract 0, and lex-bisection midpoints
                // are powers of 2 whose decoded values sit near 1.0 without
                // preserving the fractional half.
                //
                // Uses strict `sort_key`-reduction as the accept predicate so a
                // candidate that is merely interesting but lex-larger than
                // current (e.g. 0.5 vs 2.5: both satisfy fract==0.5, but
                // float_to_index(0.5) > float_to_index(2.5)) does not
                // short-circuit `bin_search_down` at k=0 and skip lex-smaller
                // values at larger k (1.5 at k=1).
                let v = self.float_at(i);
                if v.is_finite() && v != 0.0 {
                    let is_neg = v.is_sign_negative();
                    if let Some((m, n)) = as_integer_ratio(v.abs()) {
                        let k_init = m / n;
                        let r = m % n;
                        if k_init > 0 {
                            bin_search_down(0, k_init as i128, &mut |k| {
                                let num_sum = (k as u128) * n + r;
                                let candidate_abs = (num_sum as f64) / (n as f64);
                                let candidate = if is_neg {
                                    -candidate_abs
                                } else {
                                    candidate_abs
                                };
                                if !fc.validate(candidate) {
                                    return false;
                                }
                                let prev_key = sort_key(&self.current_nodes);
                                self.replace(&HashMap::from([(i, ChoiceValue::Float(candidate))]));
                                sort_key(&self.current_nodes) < prev_key
                            });
                        }
                    }
                }
            }
            i += 1;
        }
    }

    fn float_at(&self, i: usize) -> f64 {
        match self.current_nodes[i].value {
            ChoiceValue::Float(f) => f,
            _ => unreachable!("kind/value invariant violated: outer match guaranteed this variant"),
        }
    }

    /// Redistribute magnitude across nearby numeric pairs.
    ///
    /// Port of Hypothesis's `redistribute_numeric_pairs`. For sum-style
    /// constraints (`a + b > 1000`), shrinking `a` toward 0 alone breaks
    /// the predicate; the pair only collapses to its minimum when `a` is
    /// reduced and `b` is raised by the same amount in lockstep. Walks pairs
    /// `(i, j)` where `j - i` is small (cap 4 to avoid quadratic scans), at
    /// least one side is a non-trivial Float, and probes
    /// `(v_i - k, v_j + k)` (or `(v_i + k, v_j - k)` if `v_i` is below its
    /// shrink target). Maximises `k` via `find_integer`.
    ///
    /// Pure Integer-Integer pairs are already handled by
    /// [`Shrinker::redistribute_integers`] — this pass complements it by
    /// covering Float-Float, Float-Integer, and Integer-Float pairs that
    /// the integer-only pass skips.
    pub(super) fn redistribute_numeric_pairs(&mut self) {
        let len = self.current_nodes.len();
        for i in 0..len {
            for gap in 1..=4 {
                if i + gap >= self.current_nodes.len() {
                    break;
                }
                let j = i + gap;
                if !is_float_or_integer(&self.current_nodes[i].kind)
                    || !is_float_or_integer(&self.current_nodes[j].kind)
                {
                    continue;
                }
                // Skip pure Int-Int — covered by redistribute_integers.
                if matches!(
                    (&self.current_nodes[i].kind, &self.current_nodes[j].kind),
                    (ChoiceKind::Integer(_), ChoiceKind::Integer(_))
                ) {
                    continue;
                }
                // MAX_PRECISE_INTEGER guard (shrinker.py:1356-1358): for
                // a Float node, skip if the value is non-finite or has
                // `|v| >= 2^53`.  Above that magnitude `f + 1 == f` so
                // the redistribute math reads as a shrink without
                // actually reducing the value — we'd waste calls and
                // possibly accept lossy "no-op" candidates.
                if !can_choose_for_redistribute(&self.current_nodes[i])
                    || !can_choose_for_redistribute(&self.current_nodes[j])
                {
                    continue;
                }
                if is_trivial(&self.current_nodes[i]) {
                    continue;
                }
                redistribute_pair(self, i, j);
            }
        }
    }
}

/// `node.constraints["shrink_towards"]` for floats is fixed at 0 in
/// upstream and we don't carry it in [`FloatChoice`]; the only
/// node-level filter `redistribute_numeric_pairs` needs is the
/// MAX_PRECISE_INTEGER / NaN / inf check from `shrinker.py:1356-1358`.
fn can_choose_for_redistribute(node: &ChoiceNode) -> bool {
    // Caller (`redistribute_numeric_pairs`) has already filtered out
    // non-numeric kinds via `is_float_or_integer`, so anything outside
    // matched-(Int, Int) / (Float, Float) is unreachable here.
    match (&node.kind, &node.value) {
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
        ChoiceKind::Boolean(_) | ChoiceKind::Bytes(_) => false,
    }
}

fn is_trivial(node: &ChoiceNode) -> bool {
    // Only called by `redistribute_numeric_pairs` after the
    // `is_float_or_integer` filter, so Booleans cannot appear.
    match (&node.kind, &node.value) {
        (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => *v == ic.simplest(),
        (ChoiceKind::Float(fc), ChoiceValue::Float(v)) => !v.is_finite() || *v == fc.simplest(),
        _ => unreachable!("filtered by is_float_or_integer; ChoiceNode invariant otherwise"),
    }
}

/// Direction the integer-pair search moves `node[i]` in.
///
/// `v_i` is reduced toward its shrink target (0 for floats, simplest() for
/// integers); the matching adjustment to `v_j` raises it. If `v_i` is
/// already below its shrink target, both deltas flip sign.
fn redistribute_pair(shrinker: &mut Shrinker<'_>, i: usize, j: usize) {
    // Snapshot the original values; find_integer will probe larger and
    // larger `k` and the kept candidate updates current_nodes in place.
    // Caller has already filtered to Integer/Float pairs via
    // `is_float_or_integer`, and `is_trivial` rejects non-finite floats, so
    // every branch outside (Int, Int) / (Float finite, Float finite) is
    // unreachable here.
    let (v_i, kind_i) = match (
        &shrinker.current_nodes[i].kind,
        &shrinker.current_nodes[i].value,
    ) {
        (ChoiceKind::Integer(ic), ChoiceValue::Integer(n)) => {
            (NumericValue::Integer(*n), NumericKind::Integer(ic.clone()))
        }
        (ChoiceKind::Float(fc), ChoiceValue::Float(f)) => {
            (NumericValue::Float(*f), NumericKind::Float(fc.clone()))
        }
        _ => unreachable!("redistribute_pair gated on is_float_or_integer + is_trivial"),
    };
    let (v_j, kind_j) = match (
        &shrinker.current_nodes[j].kind,
        &shrinker.current_nodes[j].value,
    ) {
        (ChoiceKind::Integer(ic), ChoiceValue::Integer(n)) => {
            (NumericValue::Integer(*n), NumericKind::Integer(ic.clone()))
        }
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

    // Upstream `shrinker.py:1404-1405` guards against `cand_j` crossing
    // `MAX_PRECISE_INTEGER` here. We don't need that: the sort-key check
    // in `consider`/`replace` rejects any candidate whose `|cand_i|` grows
    // beyond the prior accept, which always trips well before `cand_j`
    // reaches `2^53`.
    find_integer(|k| {
        let (cand_i, cand_j) = apply_delta(&v_i, &v_j, k as i128, dir);
        let Some(val_i) = build_value(&kind_i, cand_i) else {
            return false;
        };
        let Some(val_j) = build_value(&kind_j, cand_j) else {
            return false;
        };
        shrinker.replace(&HashMap::from([(i, val_i), (j, val_j)]))
    });
}

#[derive(Clone, Copy)]
enum Direction {
    /// v_i above shrink target: subtract k from v_i, add k to v_j.
    LowerLeftRaiseRight,
    /// v_i below shrink target: add k to v_i, subtract k from v_j.
    RaiseLeftLowerRight,
}

#[derive(Clone, Copy)]
enum NumericValue {
    Integer(i128),
    Float(f64),
}

impl NumericValue {
    fn as_f64(self) -> f64 {
        match self {
            NumericValue::Integer(n) => n as f64,
            NumericValue::Float(f) => f,
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
        NumericKind::Integer(ic) => ic.simplest() as f64,
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
        NumericValue::Integer(n) => NumericValue::Integer(n.saturating_add(k)),
        NumericValue::Float(f) => NumericValue::Float(*f + k as f64),
    }
}

fn build_value(kind: &NumericKind, candidate: NumericValue) -> Option<ChoiceValue> {
    // `apply_delta` preserves the variant of each input, so only the
    // matching kind/value combinations are reachable from `redistribute_pair`.
    match (kind, candidate) {
        (NumericKind::Integer(ic), NumericValue::Integer(n)) => {
            ic.validate(n).then_some(ChoiceValue::Integer(n))
        }
        (NumericKind::Float(fc), NumericValue::Float(f)) => {
            fc.validate(f).then_some(ChoiceValue::Float(f))
        }
        _ => unreachable!("apply_delta preserves variant; kind and value cannot disagree"),
    }
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_floats_tests.rs"]
mod tests;
