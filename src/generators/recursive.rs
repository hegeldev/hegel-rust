use super::{Generator, TestCase, fnv1a_hash};
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const RECURSIVE_LABEL: u64 = fnv1a_hash(b"hegel::generators::recursive");

const DEFAULT_MAX_DEPTH: usize = 32;
const DEFAULT_MAX_LEAVES: usize = 100;
const BRANCH_PROBABILITY: f64 = 0.8;

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

/// The size budget for one value drawn from a [`RecursiveGenerator`]: a
/// fresh scope is created per top-level draw and shared by every
/// [`SubtreeGenerator`] taking part in that draw, so drawn leaves are
/// counted across the whole value without any state outliving the draw.
struct DrawScope {
    max_depth: usize,
    max_leaves: usize,
    leaves: AtomicUsize,
}

/// The generator a [`recursive()`] branch function receives, producing the
/// recursive sub-values of the value under construction.
///
/// Each value it generates is itself either a leaf or a further branch, with
/// the probability of branching shrinking as the value gets deeper and
/// larger. It is `Clone`, so a branch function needing several independent
/// sub-value generators (e.g. for the fields of a [`tuples!`](crate::tuples))
/// can clone it.
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

    fn draw_should_branch(&self, tc: &TestCase) -> bool {
        if self.depth >= self.scope.max_depth {
            return false;
        }
        let leaves = self.scope.leaves.load(Ordering::Relaxed);
        if leaves >= self.scope.max_leaves {
            return false;
        }
        let remaining = (self.scope.max_leaves - leaves) as f64 / self.scope.max_leaves as f64;
        let p = BRANCH_PROBABILITY.powf(self.depth as f64 + 1.0) * remaining;
        tc.generate_boolean(p)
    }
}

impl<T> Generator<T> for SubtreeGenerator<T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        tc.start_span(RECURSIVE_LABEL);
        let result = if self.draw_should_branch(tc) {
            self.core.draw_branch(tc, self.child())
        } else {
            self.scope.leaves.fetch_add(1, Ordering::Relaxed);
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

    /// Set a soft limit on the number of leaf values in one generated value
    /// (default 100).
    ///
    /// Once this many leaves have been generated, no further branches are
    /// introduced. Sub-values already begun still complete — each becoming a
    /// single leaf — so for a branch producing at most `c` sub-values the
    /// total number of leaves can exceed the limit by up to
    /// `max_depth * (c - 1)`.
    pub fn max_leaves(mut self, max_leaves: usize) -> Self {
        self.max_leaves = max_leaves;
        self
    }
}

impl<T> Generator<T> for RecursiveGenerator<T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let root = SubtreeGenerator {
            core: Arc::clone(&self.core),
            scope: Arc::new(DrawScope {
                max_depth: self.max_depth,
                max_leaves: self.max_leaves,
                leaves: AtomicUsize::new(0),
            }),
            depth: 0,
        };
        root.do_draw(tc)
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
/// so on, with the probability of further branching falling off as a value
/// gets deeper and larger. Use
/// [`max_depth`](RecursiveGenerator::max_depth) and
/// [`max_leaves`](RecursiveGenerator::max_leaves) to bound how large values
/// can grow.
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
