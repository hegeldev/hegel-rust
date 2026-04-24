//! Port of `hypothesis.internal.conjecture.shrinking.choicetree`.
//!
//! [`ChoiceTree`] records the sequences of choices made by shrink passes so
//! that a pass can track which parts of its search space have already been
//! explored. The test harness (`tests/hypothesis/conjecture_choice_tree.rs`)
//! is the only current consumer.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rand::{Rng, RngExt};

/// Selection order: given the current depth in the tree and the fan-out `n`
/// at this node, return the child indices (`0..n`) in the order they should
/// be tried.
pub type SelectionOrder = Box<dyn FnMut(usize, usize) -> Vec<usize>>;

/// Select choices starting from ``prefix``, preferring to move left then
/// wrapping around to the right.
pub fn prefix_selection_order(prefix: &[usize]) -> SelectionOrder {
    let prefix = prefix.to_vec();
    Box::new(move |depth: usize, n: usize| -> Vec<usize> {
        if n == 0 {
            return Vec::new();
        }
        let i = if depth < prefix.len() {
            prefix[depth].min(n - 1)
        } else {
            n - 1
        };
        let mut out = Vec::with_capacity(n);
        for k in (0..=i).rev() {
            out.push(k);
        }
        for k in ((i + 1)..n).rev() {
            out.push(k);
        }
        out
    })
}

/// Select children uniformly at random, yielding each of `0..n` exactly once.
pub fn random_selection_order<R: Rng + 'static>(rng: Rc<RefCell<R>>) -> SelectionOrder {
    Box::new(move |_depth: usize, n: usize| -> Vec<usize> {
        let mut pending: Vec<usize> = (0..n).collect();
        let mut result = Vec::with_capacity(n);
        let mut rng = rng.borrow_mut();
        while !pending.is_empty() {
            // Match Python `LazySequenceCopy.pop(i)` (swap-with-last pop).
            let idx = rng.random_range(0..pending.len());
            let last = pending.len() - 1;
            pending.swap(idx, last);
            result.push(pending.pop().unwrap());
        }
        result
    })
}

/// Returned from [`Chooser::choose`] when every remaining child of the
/// current node is either exhausted or rejected by the condition. The
/// caller propagates it back out of [`ChoiceTree::step`].
#[derive(Debug, Clone, Copy)]
pub struct DeadBranch;

#[derive(Clone)]
struct TreeNode(Rc<RefCell<TreeNodeInner>>);

struct TreeNodeInner {
    children: HashMap<usize, TreeNode>,
    live_child_count: Option<usize>,
    n: Option<usize>,
}

impl TreeNode {
    fn new() -> Self {
        TreeNode(Rc::new(RefCell::new(TreeNodeInner {
            children: HashMap::new(),
            live_child_count: None,
            n: None,
        })))
    }

    fn dead() -> Self {
        let node = Self::new();
        node.0.borrow_mut().live_child_count = Some(0);
        node
    }

    fn exhausted(&self) -> bool {
        self.0.borrow().live_child_count == Some(0)
    }

    /// Mirror Python's `defaultdict(TreeNode)` — `children[i]` materialises
    /// a fresh `TreeNode` on first access.
    fn child(&self, i: usize) -> TreeNode {
        self.0
            .borrow_mut()
            .children
            .entry(i)
            .or_insert_with(TreeNode::new)
            .clone()
    }

    fn kill_child(&self, i: usize) {
        let mut inner = self.0.borrow_mut();
        inner.children.insert(i, TreeNode::dead());
        let old = inner.live_child_count.expect("live_child_count set");
        inner.live_child_count = Some(old - 1);
    }
}

/// A source of nondeterminism for shrink passes.
pub struct Chooser {
    selection_order: SelectionOrder,
    node_trail: Vec<TreeNode>,
    choices: Vec<usize>,
}

impl Chooser {
    fn new(root: TreeNode, selection_order: SelectionOrder) -> Self {
        Chooser {
            selection_order,
            node_trail: vec![root],
            choices: Vec::new(),
        }
    }

    /// Return some element of `values` satisfying `condition` that does not
    /// lead to an exhausted branch. Returns [`DeadBranch`] when no such
    /// element exists.
    pub fn choose<T, F>(&mut self, values: &[T], condition: F) -> Result<T, DeadBranch>
    where
        T: Clone,
        F: Fn(&T) -> bool,
    {
        let node = self.node_trail.last().unwrap().clone();

        {
            let mut inner = node.0.borrow_mut();
            if inner.live_child_count.is_none() {
                inner.live_child_count = Some(values.len());
                inner.n = Some(values.len());
            }
            let lcc = inner.live_child_count.unwrap();
            assert!(lcc > 0 || values.is_empty());
        }

        let order = (self.selection_order)(self.choices.len(), values.len());
        for i in order {
            if node.0.borrow().live_child_count == Some(0) {
                break;
            }
            let child = node.child(i);
            if !child.exhausted() {
                let v = values[i].clone();
                if condition(&v) {
                    self.choices.push(i);
                    self.node_trail.push(child);
                    return Ok(v);
                } else {
                    node.kill_child(i);
                }
            }
        }
        assert_eq!(node.0.borrow().live_child_count, Some(0));
        Err(DeadBranch)
    }

    fn finish(mut self) -> Vec<usize> {
        let result = self.choices.clone();
        self.node_trail
            .last()
            .unwrap()
            .0
            .borrow_mut()
            .live_child_count = Some(0);
        while self.node_trail.len() > 1 && self.node_trail.last().unwrap().exhausted() {
            self.node_trail.pop();
            let i = self.choices.pop().unwrap();
            let target = self.node_trail.last().unwrap().clone();
            target.kill_child(i);
        }
        result
    }
}

/// Records sequences of choices made during shrinking so that we can track
/// what parts of a pass have run. Creates [`Chooser`] objects for passes to
/// use.
pub struct ChoiceTree {
    root: TreeNode,
}

impl Default for ChoiceTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ChoiceTree {
    pub fn new() -> Self {
        ChoiceTree {
            root: TreeNode::new(),
        }
    }

    pub fn exhausted(&self) -> bool {
        self.root.exhausted()
    }

    /// Run one pass, invoking `f` with a fresh [`Chooser`]. Returns the
    /// choice-index prefix recorded during the run.
    pub fn step<F>(&mut self, selection_order: SelectionOrder, f: F) -> Vec<usize>
    where
        F: FnOnce(&mut Chooser) -> Result<(), DeadBranch>,
    {
        assert!(!self.exhausted());
        let mut chooser = Chooser::new(self.root.clone(), selection_order);
        let _ = f(&mut chooser);
        chooser.finish()
    }
}
