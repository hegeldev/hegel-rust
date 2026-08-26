use super::{Generator, TestCase, fnv1a_hash};
use crate::control::raise_control;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const RECURSIVE_LABEL: u64 = fnv1a_hash(b"hegel::generators::recursive");

const DEFAULT_MAX_DEPTH: usize = 32;
const DEFAULT_MAX_LEAVES: usize = 100;
const MAX_ATTEMPTS: usize = 9;

/// The branch probability for the given generation attempt: `1 / (attempt + 2)`,
/// the critical probability for a tree whose branches have `attempt + 2`
/// children each. A branching process at its critical probability stays
/// finite while covering a heavy-tailed spread of sizes, so the first
/// attempt prices branches as if the tree were binary; each retry assumes
/// one more child per branch, so branch functions that actually produce
/// many children quickly reach a probability at which they fit inside
/// `max_leaves`. The probability is fixed for the whole attempt — scaling
/// it by the budget already spent would make earlier (left) subtrees
/// systematically branchier than later ones.
fn branch_probability(attempt: usize) -> f64 {
    1.0 / (attempt as f64 + 2.0)
}

/// Control payload unwound when a generation attempt draws more than
/// `max_leaves` leaves. Caught by `RecursiveGenerator::do_draw`, which
/// discards the attempt's spans and retries with a lower branch probability.
struct LeafBudgetExceeded;

/// The leaf generator and branch function of a [`recursive()`] generator,
/// type-erased so that [`SubtreeGenerator`] (which appears in the branch
/// function's own signature) does not need to name their types.
trait SubtreeDraw<T>: Send + Sync {
    fn draw_leaf(&self, tc: &TestCase) -> T;
    fn draw_branch(&self, tc: &TestCase, subtrees: SubtreeGenerator<T>) -> T;
}

struct RecursiveCore<G, F, R> {
    leaf: G,
    branch: F,
    _phantom: PhantomData<fn() -> R>,
}

impl<T, G, F, R> SubtreeDraw<T> for RecursiveCore<G, F, R>
where
    G: Generator<T> + Send + Sync,
    F: Fn(SubtreeGenerator<T>) -> R + Send + Sync,
    R: Generator<T>,
{
    fn draw_leaf(&self, tc: &TestCase) -> T {
        self.leaf.do_draw(tc)
    }

    fn draw_branch(&self, tc: &TestCase, subtrees: SubtreeGenerator<T>) -> T {
        (self.branch)(subtrees).do_draw(tc)
    }
}

/// The state of one generation attempt: a fresh scope is created per attempt
/// and shared by every [`SubtreeGenerator`] taking part in it, so drawn
/// leaves are counted across the whole value without any state outliving
/// the draw call.
struct DrawScope {
    max_depth: usize,
    max_leaves: usize,
    branch_probability: f64,
    leaves: AtomicUsize,
}

/// The generator a [`recursive()`] branch function receives, producing the
/// recursive sub-values of the value under construction.
///
/// Each value it generates is itself either a leaf or a further branch. It
/// is `Clone`, so a branch function needing several independent sub-value
/// generators (e.g. for the fields of a [`tuples!`](crate::tuples)) can
/// clone it.
pub struct SubtreeGenerator<T> {
    core: Arc<dyn SubtreeDraw<T>>,
    scope: Arc<DrawScope>,
    depth: usize,
}

impl<T> Clone for SubtreeGenerator<T> {
    fn clone(&self) -> Self {
        SubtreeGenerator {
            core: Arc::clone(&self.core),
            scope: Arc::clone(&self.scope),
            depth: self.depth,
        }
    }
}

impl<T> SubtreeGenerator<T> {
    fn child(&self) -> Self {
        SubtreeGenerator {
            core: Arc::clone(&self.core),
            scope: Arc::clone(&self.scope),
            depth: self.depth + 1,
        }
    }

    /// At the depth limit the decision is still drawn, with probability
    /// zero: the engine records it as a forced choice, so the choice
    /// sequence has the same shape whether or not the limit was hit and the
    /// shrinker can move subtrees across the boundary without misaligning
    /// every draw that follows.
    fn draw_should_branch(&self, tc: &TestCase) -> bool {
        let p = if self.depth >= self.scope.max_depth {
            0.0
        } else {
            self.scope.branch_probability
        };
        tc.generate_boolean(p)
    }
}

impl<T> Generator<T> for SubtreeGenerator<T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        tc.start_span(RECURSIVE_LABEL);
        let result = if self.draw_should_branch(tc) {
            self.core.draw_branch(tc, self.child())
        } else {
            if self.scope.leaves.fetch_add(1, Ordering::Relaxed) >= self.scope.max_leaves {
                raise_control(LeafBudgetExceeded);
            }
            self.core.draw_leaf(tc)
        };
        tc.stop_span(false);
        result
    }
}

/// Generator for recursively defined data. Created by [`recursive()`].
pub struct RecursiveGenerator<T> {
    core: Arc<dyn SubtreeDraw<T>>,
    max_depth: usize,
    max_leaves: usize,
}

impl<T> RecursiveGenerator<T> {
    /// Set the maximum nesting depth of branches (default 32).
    ///
    /// Sub-values at this depth are always leaves, so a `max_depth` of 0
    /// generates only leaves.
    pub fn max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set the maximum number of leaf values in one generated value
    /// (default 100).
    ///
    /// A generation attempt that draws more than `max_leaves` leaves is
    /// discarded and retried with a lower branching probability; if several
    /// retries in a row fail to fit, the test case is rejected as if by
    /// [`assume`](crate::TestCase::assume).
    pub fn max_leaves(mut self, max_leaves: usize) -> Self {
        self.max_leaves = max_leaves;
        self
    }
}

impl<T> Generator<T> for RecursiveGenerator<T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let base_span_depth = tc.open_span_depth();
        for attempt in 0..MAX_ATTEMPTS {
            let root = SubtreeGenerator {
                core: Arc::clone(&self.core),
                scope: Arc::new(DrawScope {
                    max_depth: self.max_depth,
                    max_leaves: self.max_leaves,
                    branch_probability: branch_probability(attempt),
                    leaves: AtomicUsize::new(0),
                }),
                depth: 0,
            };
            match catch_unwind(AssertUnwindSafe(|| root.do_draw(tc))) {
                Ok(value) => return value,
                Err(payload) if payload.downcast_ref::<LeafBudgetExceeded>().is_some() => {
                    while tc.open_span_depth() > base_span_depth {
                        tc.stop_span(true);
                    }
                }
                Err(payload) => resume_unwind(payload),
            }
        }
        tc.reject()
    }
}

/// Generate recursively defined data, such as trees or JSON documents.
///
/// `leaf` generates the non-recursive base cases. `branch` builds one level
/// of recursive structure: it receives a [`SubtreeGenerator`] producing
/// sub-values of the same type and returns a generator that combines some
/// number of them into a compound value, e.g. by collecting them with
/// [`vecs()`](super::vecs) and mapping the result into a node type. It is
/// called afresh for each branch node generated.
///
/// Generated values are leaves or branches of leaves, branches of those, and
/// so on. Sizes vary from a single leaf up to the limits set by
/// [`max_depth`](RecursiveGenerator::max_depth) (a hard depth cap) and
/// [`max_leaves`](RecursiveGenerator::max_leaves) (attempts that draw more
/// leaves than this are discarded and retried with a lower branching
/// probability).
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self as gs, Generator};
///
/// #[derive(Debug)]
/// enum Json {
///     Number(f64),
///     Array(Vec<Json>),
/// }
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let value = tc.draw(gs::recursive(
///         gs::floats::<f64>().map(Json::Number),
///         |json| gs::vecs(json).max_size(5).map(Json::Array),
///     ));
/// }
/// ```
pub fn recursive<T, G, F, R>(leaf: G, branch: F) -> RecursiveGenerator<T>
where
    T: 'static,
    G: Generator<T> + Send + Sync + 'static,
    F: Fn(SubtreeGenerator<T>) -> R + Send + Sync + 'static,
    R: Generator<T> + 'static,
{
    RecursiveGenerator {
        core: Arc::new(RecursiveCore {
            leaf,
            branch,
            _phantom: PhantomData,
        }),
        max_depth: DEFAULT_MAX_DEPTH,
        max_leaves: DEFAULT_MAX_LEAVES,
    }
}
