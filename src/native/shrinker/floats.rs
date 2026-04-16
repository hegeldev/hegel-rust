// Float shrink pass: shrink_floats.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue, float_to_index, index_to_float};

use super::{Shrinker, bin_search_down};

impl<'a> Shrinker<'a> {
    /// Shrink float choices toward simpler values using Hypothesis lex ordering.
    ///
    /// Steps per float node:
    /// 1. Try replacing with simplest().
    /// 2. If sign-negative, try negating (positive is simpler).
    /// 3. Binary search on absolute-value lex index from 0 toward current value.
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

                // Re-read current value.
                let v = {
                    let Some(n) = self.current_nodes.get(i) else { break; };
                    let ChoiceValue::Float(f) = n.value else { i += 1; continue; };
                    f
                };

                // Skip NaN — can't binary search on NaN.
                if v.is_nan() {
                    i += 1;
                    continue;
                }

                // Step 2: Try negating if sign-negative (positive is simpler).
                if v.is_sign_negative() {
                    let neg = -v;
                    if fc.validate(neg) {
                        self.replace(&HashMap::from([(i, ChoiceValue::Float(neg))]));
                    }
                }

                // Re-read after possible negation.
                let v = {
                    let Some(n) = self.current_nodes.get(i) else { break; };
                    let ChoiceValue::Float(f) = n.value else { i += 1; continue; };
                    f
                };

                if v.is_nan() {
                    i += 1;
                    continue;
                }

                // Step 3a: When current is a non-integer, explicitly search the
                // integer-float range.  In our ordering, integer floats 0, 1, 2, …
                // have indices 0, 1, 2, … (much smaller than any non-integer).
                // The existing binary search below misses them because it jumps
                // past small indices when hi is near 2^63.
                let v_abs = v.abs();
                let current_idx = float_to_index(v_abs);
                let is_neg = v.is_sign_negative();
                if current_idx >= (1u64 << 63) {
                    // Compute the integer magnitude range valid for fc.
                    let (int_lo, int_hi) = if is_neg {
                        // Negative float: candidate = -(n as f64).
                        // Valid when -(n as f64) ∈ [fc.min_value, fc.max_value],
                        // i.e. n ∈ [-fc.max_value, -fc.min_value].
                        let lo = if fc.max_value <= 0.0 {
                            (-fc.max_value).ceil() as i128
                        } else {
                            0
                        };
                        let hi = if fc.min_value <= 0.0 {
                            (-fc.min_value).floor() as i128
                        } else {
                            -1
                        };
                        (lo, hi)
                    } else {
                        let lo = fc.min_value.max(0.0).ceil() as i128;
                        let hi = fc.max_value.min(f64::MAX).floor() as i128;
                        (lo, hi)
                    };
                    if int_lo >= 0 && int_lo <= int_hi {
                        let i_capture = i;
                        let is_neg_capture = is_neg;
                        let fc_capture = fc.clone();
                        bin_search_down(int_lo, int_hi, &mut |n| {
                            let candidate =
                                if is_neg_capture { -(n as f64) } else { n as f64 };
                            if fc_capture.validate(candidate) {
                                self.replace(&HashMap::from([(
                                    i_capture,
                                    ChoiceValue::Float(candidate),
                                )]))
                            } else {
                                false
                            }
                        });
                    }
                }

                // Re-read current value after possible integer replacement.
                let v = {
                    let Some(n) = self.current_nodes.get(i) else { break; };
                    let ChoiceValue::Float(f) = n.value else { i += 1; continue; };
                    f
                };
                if v.is_nan() {
                    i += 1;
                    continue;
                }

                // Step 3b: Binary search on absolute-value lex index toward 0.
                // float_to_index handles both finite and infinite non-NaN non-negative floats.
                let v_abs = v.abs();
                let current_idx = float_to_index(v_abs);
                let is_neg = v.is_sign_negative();
                if current_idx > 0 {
                    bin_search_down(0, current_idx as i128, &mut |idx| {
                        let candidate_mag = index_to_float(idx as u64);
                        let candidate = if is_neg { -candidate_mag } else { candidate_mag };
                        if fc.validate(candidate) {
                            self.replace(&HashMap::from([(
                                i,
                                ChoiceValue::Float(candidate),
                            )]))
                        } else {
                            false
                        }
                    });
                }
            }
            i += 1;
        }
    }
}
