// Shrinker for the native backend.
//
// Ported from pbtkit core.py. Reduces failing test cases to minimal
// counterexamples by systematically simplifying the choice sequence.

use std::collections::HashMap;

use crate::native::core::{
    ChoiceKind, ChoiceNode, ChoiceValue, NodeSortKey,
    MAX_SHRINK_ITERATIONS, sort_key,
};

/// A callback that runs a test case from a choice sequence and returns
/// whether it was interesting (i.e. the test failed).
pub type TestFn<'a> = dyn FnMut(&[ChoiceNode]) -> bool + 'a;

pub struct Shrinker<'a> {
    test_fn: Box<TestFn<'a>>,
    pub current_nodes: Vec<ChoiceNode>,
}

impl<'a> Shrinker<'a> {
    pub fn new(
        test_fn: Box<TestFn<'a>>,
        initial_nodes: Vec<ChoiceNode>,
    ) -> Self {
        Shrinker {
            test_fn,
            current_nodes: initial_nodes,
        }
    }

    /// Try a candidate choice sequence. If interesting and smaller than
    /// the current best, update current_nodes. Returns whether interesting.
    pub fn consider(&mut self, nodes: &[ChoiceNode]) -> bool {
        if sort_key(nodes) == sort_key(&self.current_nodes) {
            return true;
        }
        let is_interesting = (self.test_fn)(nodes);
        if is_interesting && sort_key(nodes) < sort_key(&self.current_nodes) {
            self.current_nodes = nodes.to_vec();
        }
        is_interesting
    }

    /// Try replacing values at specific indices.
    pub fn replace(&mut self, values: &HashMap<usize, ChoiceValue>) -> bool {
        let mut attempt: Vec<ChoiceNode> = self.current_nodes.clone();
        for (&i, v) in values {
            assert!(i < attempt.len());
            attempt[i] = attempt[i].with_value(v.clone());
        }
        self.consider(&attempt)
    }

    /// Run all shrink passes repeatedly until no more progress or iteration cap.
    pub fn shrink(&mut self) {
        let mut prev: Vec<NodeSortKey> = Vec::new();
        let mut iterations = 0;

        loop {
            let current_key: Vec<NodeSortKey> =
                self.current_nodes.iter().map(|n| n.sort_key()).collect();
            if current_key == prev || iterations >= MAX_SHRINK_ITERATIONS {
                break;
            }
            prev = current_key;
            iterations += 1;

            self.delete_chunks();
            self.zero_choices();
            self.swap_integer_sign();
            self.binary_search_integer_towards_zero();
        }
    }

    /// Try deleting chunks of choices from the sequence.
    ///
    /// Longer chunks allow deleting composite elements (e.g. a list element
    /// requires deleting both the "include?" choice and the element itself).
    /// Iterates backwards since later choices tend to depend on earlier ones.
    fn delete_chunks(&mut self) {
        let mut k: usize = 8;
        while k > 0 {
            let mut i = self.current_nodes.len().saturating_sub(k + 1);
            loop {
                if i >= self.current_nodes.len() {
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                    continue;
                }
                let end = (i + k).min(self.current_nodes.len());
                let mut attempt: Vec<ChoiceNode> = self.current_nodes[..i].to_vec();
                attempt.extend_from_slice(&self.current_nodes[end..]);
                assert!(attempt.len() < self.current_nodes.len());

                if !self.consider(&attempt) {
                    // Try decrementing the preceding choice (helps with
                    // collection length counters).
                    if i > 0 {
                        let prev = &attempt[i - 1];
                        if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) =
                            (&prev.kind, &prev.value)
                        {
                            if *v != ic.simplest() {
                                let mut modified = attempt.clone();
                                modified[i - 1] =
                                    modified[i - 1].with_value(ChoiceValue::Integer(v - 1));
                                if self.consider(&modified) {
                                    if i == 0 {
                                        break;
                                    }
                                    i -= 1;
                                    continue;
                                }
                            }
                        }
                        if let (ChoiceKind::Boolean(_), ChoiceValue::Boolean(true)) =
                            (&prev.kind, &prev.value)
                        {
                            let mut modified = attempt.clone();
                            modified[i - 1] =
                                modified[i - 1].with_value(ChoiceValue::Boolean(false));
                            if self.consider(&modified) {
                                if i == 0 {
                                    break;
                                }
                                i -= 1;
                                continue;
                            }
                        }
                    }
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                } else if i == 0 {
                    break;
                } else {
                    i -= 1;
                }
            }
            k -= 1;
        }
    }

    /// Replace blocks of choices with their simplest values.
    fn zero_choices(&mut self) {
        let mut k = self.current_nodes.len();
        while k > 0 {
            let mut i = 0;
            while i + k <= self.current_nodes.len() {
                let nodes = &self.current_nodes;
                if nodes[i].value == nodes[i].kind.simplest() {
                    i += 1;
                } else {
                    let replacements: HashMap<usize, ChoiceValue> = (i..i + k)
                        .map(|j| (j, self.current_nodes[j].kind.simplest()))
                        .collect();
                    self.replace(&replacements);
                    i += k;
                }
            }
            k /= 2;
        }
    }

    /// For integer choices: try simplest, then flip negative to positive.
    fn swap_integer_sign(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) = (&node.kind, &node.value) {
                let v = *v;
                if v != ic.simplest() {
                    self.replace(&HashMap::from([(i, ChoiceValue::Integer(ic.simplest()))]));
                }
                // Re-read in case the replace changed things
                if i < self.current_nodes.len() {
                    if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) =
                        (&self.current_nodes[i].kind, &self.current_nodes[i].value)
                    {
                        if *v < 0 && ic.validate(-*v) {
                            self.replace(&HashMap::from([(i, ChoiceValue::Integer(-*v))]));
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// Binary search integer values toward zero.
    fn binary_search_integer_towards_zero(&mut self) {
        let mut i = 0;
        while i < self.current_nodes.len() {
            let node = &self.current_nodes[i];
            if let (ChoiceKind::Integer(ic), ChoiceValue::Integer(v)) = (&node.kind, &node.value) {
                let v = *v;
                let ic = ic.clone();
                if v > 0 {
                    let lo = ic.simplest().max(0);
                    bin_search_down(lo, v, &mut |candidate| {
                        self.replace(&HashMap::from([(i, ChoiceValue::Integer(candidate))]))
                    });
                } else if v < 0 {
                    let lo = ic.simplest().min(0).abs();
                    bin_search_down(lo, -v, &mut |candidate| {
                        self.replace(&HashMap::from([(i, ChoiceValue::Integer(-candidate))]))
                    });
                }
            }
            i += 1;
        }
    }
}

/// Binary search for the smallest value in [lo, hi] where f returns true.
///
/// Assumes f(hi) is true (not checked). Returns lo if f(lo) is true,
/// otherwise finds a locally minimal true value.
fn bin_search_down(lo: i128, hi: i128, f: &mut impl FnMut(i128) -> bool) -> i128 {
    if f(lo) {
        return lo;
    }
    let mut lo = lo;
    let mut hi = hi;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if f(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}
