// Deletion-based shrink passes: delete_chunks, bind_deletion, try_replace_with_deletion.

use std::collections::HashMap;

use crate::native::core::{ChoiceKind, ChoiceValue};

use super::{Shrinker, bin_search_down};

impl<'a> Shrinker<'a> {
    /// Try deleting chunks of choices from the sequence.
    ///
    /// Longer chunks allow deleting composite elements (e.g. a list element
    /// requires deleting both the "include?" choice and the element itself).
    /// Iterates backwards since later choices tend to depend on earlier ones.
    pub(super) fn delete_chunks(&mut self) {
        let mut k: usize = 8;
        while k > 0 {
            let mut i = self.current_nodes.len().saturating_sub(k + 1);
            loop {
                // Only reached when a prior iteration shrank current_nodes to
                // empty; with usize i we can't go negative, so we bail.
                if i >= self.current_nodes.len() {
                    break;
                }
                let end = (i + k).min(self.current_nodes.len());
                let mut attempt: Vec<_> = self.current_nodes[..i].to_vec();
                attempt.extend_from_slice(&self.current_nodes[end..]);
                assert!(attempt.len() < self.current_nodes.len());

                if !self.consider(&attempt) && i > 0 {
                    // Try decrementing the preceding choice (helps with
                    // collection length counters).
                    let prev = &attempt[i - 1];
                    let decremented = match (&prev.kind, &prev.value) {
                        (ChoiceKind::Integer(ic), ChoiceValue::Integer(v))
                            if *v != ic.simplest() =>
                        {
                            Some(ChoiceValue::Integer(v - 1))
                        }
                        (ChoiceKind::Boolean(_), ChoiceValue::Boolean(true)) => {
                            Some(ChoiceValue::Boolean(false))
                        }
                        _ => None,
                    };
                    if let Some(new_value) = decremented {
                        let mut modified = attempt.clone();
                        modified[i - 1] = modified[i - 1].with_value(new_value);
                        self.consider(&modified);
                    }
                }
                if i == 0 {
                    break;
                }
                i -= 1;
            }
            k -= 1;
        }
    }

    /// Port of pbtkit's `bind_deletion`.
    ///
    /// When a value controls the length of a downstream sequence (e.g.
    /// via flat_map), reducing that value may shorten the test case without
    /// keeping the result interesting. This pass detects that situation and
    /// tries deleting the now-excess choices to recover an interesting result.
    pub(super) fn bind_deletion(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = self.current_nodes[i].clone();

            // Only process integer nodes — these control sequence lengths.
            let (current_val, ic) = match (&node.kind, &node.value) {
                (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) => (*v, ic.clone()),
                _ => {
                    i += 1;
                    continue;
                }
            };

            let simplest = ic.simplest();
            if current_val == simplest {
                i += 1;
                continue;
            }

            let expected_len = self.current_nodes.len();

            // Binary-search smaller integer values; for each candidate, try
            // replace-with-deletion.
            let changed = bin_search_down(simplest, current_val, &mut |v| {
                self.try_replace_with_deletion(i, ChoiceValue::Integer(v), expected_len)
            });
            let _ = changed;

            i += 1;
        }
    }

    /// Try replacing the value at `idx`. If the result is interesting, done.
    /// If the result is valid but used fewer nodes than `expected_len`, try
    /// deleting regions after `idx` to recover an interesting result.
    pub(super) fn try_replace_with_deletion(
        &mut self,
        idx: usize,
        value: ChoiceValue,
        expected_len: usize,
    ) -> bool {
        // First try a straight replace. consider() already calls test_fn and
        // records the interesting case; we'd just duplicate work by retrying.
        if self.replace(&HashMap::from([(idx, value.clone())])) {
            return true;
        }

        // The replace couldn't narrow the result directly. Re-run the test to
        // see how many nodes it consumed — if fewer than expected, the trailing
        // choices may be deletable. replace() asserted idx < current_nodes.len()
        // and, since it returned false, did not mutate current_nodes, so idx is
        // still in range here.
        let mut attempt = self.current_nodes.clone();
        attempt[idx] = attempt[idx].with_value(value);

        let (_, actual_len) = (self.test_fn)(&attempt);
        if actual_len >= expected_len {
            return false;
        }

        // The test used fewer nodes. Try deleting regions after idx.
        let k = expected_len - actual_len;
        for size in (1..=k).rev() {
            let start = attempt.len().saturating_sub(size);
            if start <= idx {
                continue;
            }
            for j in (idx + 1..=start).rev() {
                let mut candidate = attempt[..j].to_vec();
                candidate.extend_from_slice(&attempt[j + size..]);
                if self.consider(&candidate) {
                    return true;
                }
            }
        }
        false
    }
}
