use super::{Generator, PrintableGenerator, TestCase};
use crate::control::{AttemptMispriced, LeafBudgetExceeded, raise_control};
use crate::ffi::RecursionHandle;
use crate::pretty::PrettyPrinter;
use crate::test_case::{labels, raise_for_rc};
use hegel_c::hegel_result_t;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;

const DEFAULT_MAX_DEPTH: usize = 32;
const DEFAULT_MAX_LEAVES: usize = 100;

/// The leaf generator and branch function of a [`recursive()`] generator,
/// type-erased so that [`SubtreeGenerator`] (which appears in the branch
/// function's own signature) does not need to name their types. The printer
/// threads through both methods so one erased object serves both draw
/// paths; the silent path passes the no-op printer.
trait SubtreeDraw<T>: Send + Sync {
    fn draw_leaf(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T;
    fn draw_branch(
        &self,
        tc: &TestCase,
        subtrees: SubtreeGenerator<T>,
        printer: &mut PrettyPrinter,
    ) -> T;
}

/// The erased core of a silent draw: ignores the printer and draws the leaf
/// and branch generators through [`Generator::do_draw`].
struct SilentCore<G, F, R> {
    leaf: Arc<G>,
    branch: Arc<F>,
    _phantom: PhantomData<fn() -> R>,
}

impl<T, G, F, R> SubtreeDraw<T> for SilentCore<G, F, R>
where
    G: Generator<T> + Send + Sync,
    F: Fn(SubtreeGenerator<T>) -> R + Send + Sync,
    R: Generator<T>,
{
    fn draw_leaf(&self, tc: &TestCase, _printer: &mut PrettyPrinter) -> T {
        self.leaf.do_draw(tc)
    }

    fn draw_branch(
        &self,
        tc: &TestCase,
        subtrees: SubtreeGenerator<T>,
        _printer: &mut PrettyPrinter,
    ) -> T {
        (self.branch)(subtrees).do_draw(tc)
    }
}

/// The erased core of a printing draw: draws the leaf and branch generators
/// through [`TestCase::draw_and_print`], so each value prints with its own
/// generator's representation.
struct PrintingCore<G, F, R> {
    leaf: Arc<G>,
    branch: Arc<F>,
    _phantom: PhantomData<fn() -> R>,
}

impl<T, G, F, R> SubtreeDraw<T> for PrintingCore<G, F, R>
where
    G: PrintableGenerator<T> + Send + Sync,
    F: Fn(SubtreeGenerator<T>) -> R + Send + Sync,
    R: PrintableGenerator<T>,
{
    fn draw_leaf(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        tc.draw_and_print(&*self.leaf, printer)
    }

    fn draw_branch(
        &self,
        tc: &TestCase,
        subtrees: SubtreeGenerator<T>,
        printer: &mut PrettyPrinter,
    ) -> T {
        tc.draw_and_print((self.branch)(subtrees), printer)
    }
}

/// The generator a [`recursive()`] branch function receives, producing the
/// recursive sub-values of the value under construction.
///
/// Each value it generates is itself either a leaf or a further branch. It
/// is `Clone`, so a branch function needing several sub-value generators
/// (e.g. for the fields of a [`tuples!`](crate::tuples)) can clone it.
/// Cloning is needed rather than borrowing (`tuples!(&subtrees, &subtrees)`)
/// because the generator the branch function returns would otherwise borrow
/// the function's own parameter.
///
/// It is a [`PrintableGenerator`], so branch functions can build on
/// printable combinators and draw sub-values with
/// [`draw`](crate::TestCase::draw); each sub-value actually prints exactly
/// when the whole recursive generator does, with the leaf and branch
/// generators' own representations.
pub struct SubtreeGenerator<T> {
    core: Arc<dyn SubtreeDraw<T>>,
    recursion: Arc<RecursionHandle>,
    depth: u64,
}

impl<T> Clone for SubtreeGenerator<T> {
    fn clone(&self) -> Self {
        SubtreeGenerator {
            core: Arc::clone(&self.core),
            recursion: Arc::clone(&self.recursion),
            depth: self.depth,
        }
    }
}

impl<T> SubtreeGenerator<T> {
    fn child(&self) -> Self {
        SubtreeGenerator {
            core: Arc::clone(&self.core),
            recursion: Arc::clone(&self.recursion),
            depth: self.depth + 1,
        }
    }

    /// The one leaf-or-branch body both draw paths run; the silent path
    /// passes the no-op printer.
    fn draw_subtree(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        tc.start_span(labels::RECURSIVE);
        let branch = match tc.with_ctc(|ctc| ctc.recursion_branch(&self.recursion, self.depth)) {
            Ok(branch) => branch,
            Err(rc) => raise_for_rc(rc),
        };
        let result = if branch {
            self.core.draw_branch(tc, self.child(), printer)
        } else {
            if let Err(rc) = tc.with_ctc(|ctc| ctc.recursion_leaf(&self.recursion)) {
                raise_for_rc(rc);
            }
            self.core.draw_leaf(tc, printer)
        };
        if self.depth == 0 {
            if let Err(rc) = tc.with_ctc(|ctc| ctc.recursion_finish(&self.recursion)) {
                if rc == hegel_result_t::HEGEL_E_RETRY {
                    raise_control(AttemptMispriced);
                }
                raise_for_rc(rc);
            }
        }
        tc.stop_span(false);
        result
    }
}

impl<T> Generator<T> for SubtreeGenerator<T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        self.draw_subtree(tc, &mut PrettyPrinter::noop())
    }
}

impl<T> PrintableGenerator<T> for SubtreeGenerator<T> {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        self.draw_subtree(tc, printer)
    }
}

/// Generator for recursively defined data. Created by [`recursive()`].
///
/// A [`PrintableGenerator`] exactly when the leaf generator and the
/// generator the branch function returns both are; drawn values then print
/// with those generators' own representations.
pub struct RecursiveGenerator<T, G, F, R> {
    leaf: Arc<G>,
    branch: Arc<F>,
    max_depth: usize,
    max_leaves: usize,
    _phantom: PhantomData<fn() -> (T, R)>,
}

impl<T, G, F, R> RecursiveGenerator<T, G, F, R> {
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
    /// Each generated value steers toward a target size drawn from across
    /// this budget, adapting to the number of sub-values the branch
    /// function actually draws, so typical sizes span the whole budget. A
    /// generation attempt that draws more than `max_leaves` leaves is
    /// discarded and retried steering toward a smaller target; if several
    /// retries in a row fail to fit, the test case is rejected as if by
    /// [`assume`](crate::TestCase::assume).
    pub fn max_leaves(mut self, max_leaves: usize) -> Self {
        self.max_leaves = max_leaves;
        self
    }

    /// The one retry loop both draw paths run: each attempt draws inside a
    /// speculative print region, so a discarded attempt (over the leaf
    /// budget, or mispriced) discards whatever it printed.
    fn draw_recursive(
        &self,
        tc: &TestCase,
        core: Arc<dyn SubtreeDraw<T>>,
        printer: &mut PrettyPrinter,
    ) -> T {
        let base_span_depth = tc.open_span_depth();
        let recursion = match tc
            .with_ctc(|ctc| ctc.new_recursion(self.max_depth as u64, self.max_leaves as u64))
        {
            Ok(recursion) => Arc::new(recursion),
            Err(rc) => raise_for_rc(rc),
        };
        loop {
            let root = SubtreeGenerator {
                core: Arc::clone(&core),
                recursion: Arc::clone(&recursion),
                depth: 0,
            };
            let mut speculation = printer.speculate();
            match catch_unwind(AssertUnwindSafe(|| {
                root.draw_subtree(tc, speculation.printer())
            })) {
                Ok(value) => {
                    speculation.commit();
                    return value;
                }
                Err(payload) if payload.downcast_ref::<LeafBudgetExceeded>().is_some() => {
                    speculation.abort();
                    match tc.with_ctc(|ctc| ctc.recursion_retry(&recursion)) {
                        Ok(()) => tc.reset_open_spans_to(base_span_depth),
                        Err(rc) => raise_for_rc(rc),
                    }
                }
                Err(payload) if payload.downcast_ref::<AttemptMispriced>().is_some() => {
                    speculation.abort();
                    tc.reset_open_spans_to(base_span_depth);
                }
                Err(payload) => resume_unwind(payload),
            }
        }
    }
}

impl<T, G, F, R> Generator<T> for RecursiveGenerator<T, G, F, R>
where
    T: 'static,
    G: Generator<T> + Send + Sync + 'static,
    F: Fn(SubtreeGenerator<T>) -> R + Send + Sync + 'static,
    R: Generator<T> + 'static,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        let core: Arc<dyn SubtreeDraw<T>> = Arc::new(SilentCore {
            leaf: Arc::clone(&self.leaf),
            branch: Arc::clone(&self.branch),
            _phantom: PhantomData,
        });
        self.draw_recursive(tc, core, &mut PrettyPrinter::noop())
    }
}

impl<T, G, F, R> PrintableGenerator<T> for RecursiveGenerator<T, G, F, R>
where
    T: 'static,
    G: PrintableGenerator<T> + Send + Sync + 'static,
    F: Fn(SubtreeGenerator<T>) -> R + Send + Sync + 'static,
    R: PrintableGenerator<T> + 'static,
{
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        let core: Arc<dyn SubtreeDraw<T>> = Arc::new(PrintingCore {
            leaf: Arc::clone(&self.leaf),
            branch: Arc::clone(&self.branch),
            _phantom: PhantomData,
        });
        self.draw_recursive(tc, core, printer)
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
/// The result is a [`PrintableGenerator`] exactly when `leaf` and the
/// generator `branch` returns both are.
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
/// hegel::pretty_print_as_debug!(Json);
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let value = tc.draw(gs::recursive(
///         gs::floats::<f64>().map(Json::Number),
///         |json| gs::vecs(json).max_size(5).map(Json::Array),
///     ));
/// }
/// ```
pub fn recursive<T, G, F, R>(leaf: G, branch: F) -> RecursiveGenerator<T, G, F, R>
where
    T: 'static,
    G: Generator<T> + Send + Sync + 'static,
    F: Fn(SubtreeGenerator<T>) -> R + Send + Sync + 'static,
    R: Generator<T> + 'static,
{
    RecursiveGenerator {
        leaf: Arc::new(leaf),
        branch: Arc::new(branch),
        max_depth: DEFAULT_MAX_DEPTH,
        max_leaves: DEFAULT_MAX_LEAVES,
        _phantom: PhantomData,
    }
}
