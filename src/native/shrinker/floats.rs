// Float shrink pass: shrink_floats.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue, float_to_index, index_to_float};

use super::{Shrinker, bin_search_down};

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
            }
            i += 1;
        }
    }

    fn float_at(&self, i: usize) -> f64 {
        match self.current_nodes[i].value {
            ChoiceValue::Float(f) => f,
            _ => unreachable!(),
        }
    }
}
