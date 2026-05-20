//! ChoiceTree / Chooser for resumable pass execution.
//!
//! Port of `shrinking/choicetree.py`.  Each shrink pass uses a `Chooser`
//! to make `choose(values, condition)` decisions; the tree records every
//! branch taken and prunes dead branches so subsequent invocations of
//! the same pass can resume from where the previous one left off.
//!
//! The infrastructure is in place but, for now, only newly-written
//! passes are migrated to take `&mut Chooser`.  The existing native
//! passes continue to iterate directly — the scheduling layer (Step 12)
//! still calls them as plain functions.  Migrating those to `Chooser`
//! is left to a follow-up.
#![allow(dead_code)]

use std::collections::HashMap;

/// Live state for one `Chooser`.
///
/// Each node tracks how many of its children are still "live"
/// (`live_child_count`) and the count of options seen at this depth
/// (`n`).  When `live_child_count == 0` the subtree is exhausted.
#[derive(Debug, Default)]
pub struct TreeNode {
    /// Child nodes keyed by their parent-side choice index.
    pub children: HashMap<usize, TreeNode>,
    /// `Some(k)` once the parent's `Chooser` has resolved how many
    /// alternatives the parent had.  Set to `Some(0)` to mark the node
    /// as a dead branch.
    pub live_child_count: Option<usize>,
    /// Width of the parent's `Chooser::choose` call (the `n` parameter
    /// in Hypothesis's TreeNode).
    pub n: Option<usize>,
}

impl TreeNode {
    /// True iff this node has no live children.
    pub fn exhausted(&self) -> bool {
        self.live_child_count == Some(0)
    }
}

/// Tree-level root for a sequence of `Chooser` invocations.
#[derive(Debug, Default)]
pub struct ChoiceTree {
    pub root: TreeNode,
}

impl ChoiceTree {
    /// True iff every branch through the tree has been exhausted.
    pub fn exhausted(&self) -> bool {
        self.root.exhausted()
    }

    /// Run one pass step `f` over a fresh `Chooser`.  Returns the
    /// sequence of decisions made (suitable for re-running the same
    /// path next time).  Dead branches encountered during the step are
    /// pruned from the tree so the next step skips them.
    pub fn step<F>(&mut self, selection_order: SelectionOrder, f: F) -> Vec<usize>
    where
        F: FnOnce(&mut Chooser) -> Result<(), DeadBranch>,
    {
        assert!(!self.exhausted(), "stepping an exhausted tree");
        let mut chooser = Chooser::new(self, selection_order);
        let _ = f(&mut chooser);
        chooser.finish()
    }
}

/// Sentinel error type raised when a `Chooser` step hits a dead branch
/// and can't continue.
#[derive(Debug)]
pub struct DeadBranch;

/// Source of nondeterminism for a single pass step.
///
/// `chooser.choose(values, condition)` picks one of `values` that
/// hasn't already exhausted its subtree and passes `condition`, then
/// records the choice in the underlying tree.
pub struct Chooser<'a> {
    tree: &'a mut ChoiceTree,
    /// Path of node references from the root.  We store indices rather
    /// than `&mut TreeNode` to keep the borrow checker happy across
    /// `choose` calls.
    trail: Vec<Vec<usize>>,
    choices: Vec<usize>,
    selection_order: SelectionOrder,
    finished: bool,
}

/// Closure that yields candidate child indices in the order to try
/// them, given the current depth and the parent's choice cardinality.
pub type SelectionOrder = Box<dyn FnMut(usize, usize) -> Vec<usize>>;

impl<'a> Chooser<'a> {
    fn new(tree: &'a mut ChoiceTree, selection_order: SelectionOrder) -> Self {
        Chooser {
            tree,
            trail: vec![Vec::new()],
            choices: Vec::new(),
            selection_order,
            finished: false,
        }
    }

    fn node_mut(&mut self, path: &[usize]) -> &mut TreeNode {
        let mut node = &mut self.tree.root;
        for &p in path {
            node = node.children.entry(p).or_default();
        }
        node
    }

    /// Pick one of `values` that satisfies `condition` and that points
    /// at a still-live child.
    pub fn choose<T: Clone>(
        &mut self,
        values: &[T],
        mut condition: impl FnMut(&T) -> bool,
    ) -> Result<T, DeadBranch> {
        assert!(!self.finished);
        let path = self.trail.last().expect("trail never empty").clone();
        let node = self.node_mut(&path);
        if node.live_child_count.is_none() {
            node.live_child_count = Some(values.len());
            node.n = Some(values.len());
        }
        if values.is_empty() {
            // Nothing to choose from.  Mark this node exhausted.
            node.live_child_count = Some(0);
            return Err(DeadBranch);
        }

        let depth = self.choices.len();
        let order = (self.selection_order)(depth, values.len());

        for i in order {
            let parent_path = self.trail.last().expect("trail never empty").clone();
            let parent = self.node_mut(&parent_path);
            if parent.live_child_count == Some(0) {
                break;
            }
            let child_exhausted = parent.children.get(&i).is_some_and(|c| c.exhausted());
            if child_exhausted {
                continue;
            }
            if !condition(&values[i]) {
                // Mark this child dead.
                parent.children.entry(i).or_default().live_child_count = Some(0);
                let live = parent.live_child_count.unwrap_or(0).saturating_sub(1);
                parent.live_child_count = Some(live);
                continue;
            }
            self.choices.push(i);
            let mut next_path = parent_path;
            next_path.push(i);
            self.trail.push(next_path);
            return Ok(values[i].clone());
        }
        // Exhausted all alternatives at this depth.
        let parent = self.node_mut(&path);
        parent.live_child_count = Some(0);
        Err(DeadBranch)
    }

    /// Record the choices made and prune dead nodes back up the path.
    pub fn finish(mut self) -> Vec<usize> {
        self.finished = true;
        let result = self.choices.clone();
        // Mark the leaf as exhausted (one step is one path through the
        // tree).
        let leaf_path = self.trail.last().expect("trail never empty").clone();
        let leaf = self.node_mut(&leaf_path);
        leaf.live_child_count = Some(0);
        // Bubble up: while the leaf is exhausted and we have a parent,
        // pop and decrement the parent's live_child_count.
        while self.trail.len() > 1 {
            let path = self.trail.last().expect("trail never empty").clone();
            let leaf = self.node_mut(&path);
            if !leaf.exhausted() {
                break;
            }
            self.trail.pop();
            let &dead_idx = self.choices.last().expect("choice matched path");
            self.choices.pop();
            let parent_path = self.trail.last().expect("trail never empty").clone();
            let parent = self.node_mut(&parent_path);
            parent
                .children
                .entry(dead_idx)
                .or_default()
                .live_child_count = Some(0);
            let live = parent.live_child_count.unwrap_or(0).saturating_sub(1);
            parent.live_child_count = Some(live);
        }
        result
    }
}

/// Selection order that starts from `prefix`, prefers moving toward
/// zero, then wraps to the right.  Used to deterministically resume a
/// previously-explored path before searching unexplored branches.
pub fn prefix_selection_order(prefix: Vec<usize>) -> SelectionOrder {
    Box::new(move |depth: usize, n: usize| -> Vec<usize> {
        if n == 0 {
            return Vec::new();
        }
        if depth < prefix.len() {
            let i = prefix[depth].min(n - 1);
            let mut out: Vec<usize> = (0..=i).rev().collect();
            out.extend((i + 1..n).rev());
            out
        } else {
            (0..n).rev().collect()
        }
    })
}

/// Uniform-random selection order seeded by `seed`.  Used by the
/// scheduler when deterministic order has stalled.
pub fn random_selection_order(seed: u64) -> SelectionOrder {
    use rand::RngExt;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    let mut rng = SmallRng::seed_from_u64(seed);
    Box::new(move |_depth: usize, n: usize| -> Vec<usize> {
        let mut pending: Vec<usize> = (0..n).collect();
        let mut out: Vec<usize> = Vec::with_capacity(n);
        while !pending.is_empty() {
            let idx = rng.random_range(0..pending.len());
            out.push(pending.remove(idx));
        }
        out
    })
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/shrinker_choicetree_tests.rs"]
mod tests;
