use super::{BoxedGenerator, Generator};
use crate::test_case::{TestCase, invalid_argument};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::thread::ThreadId;

const RECURSIVE_LABEL: u64 = super::fnv1a_hash(b"hegel::generators::recursive");

/// Probability that a node with budget remaining expands into the extended
/// generator rather than bottoming out at the base generator. High enough
/// that generated structures routinely fill their drawn size budget.
const BRANCH_PROBABILITY: f64 = 0.9;

struct Frame {
    depth: usize,
    remaining: i64,
}

struct RecursiveState<'a, T> {
    base: BoxedGenerator<'a, T>,
    extended: OnceLock<BoxedGenerator<'a, T>>,
    max_size: i64,
    frames: Mutex<HashMap<ThreadId, Frame>>,
}

/// Removes this thread's budget frame when the outermost draw finishes,
/// including when it unwinds (a rejected assumption, out-of-data), so a
/// later top-level draw starts a fresh budget.
struct FrameGuard<'s> {
    frames: &'s Mutex<HashMap<ThreadId, Frame>>,
    thread: ThreadId,
}

impl Drop for FrameGuard<'_> {
    fn drop(&mut self) {
        let mut frames = self.frames.lock();
        let frame = frames.get_mut(&self.thread).unwrap();
        frame.depth -= 1;
        if frame.depth == 0 {
            frames.remove(&self.thread);
        }
    }
}

/// Generator for recursive structures with a size budget. Created by
/// [`recursive()`]; the handle passed to the `extend` closure is a clone of
/// the same generator, so drawing from it produces the sub-structures.
pub struct RecursiveGenerator<'a, T> {
    state: Arc<RecursiveState<'a, T>>,
}

impl<T> Clone for RecursiveGenerator<'_, T> {
    fn clone(&self) -> Self {
        RecursiveGenerator {
            state: Arc::clone(&self.state),
        }
    }
}

impl<'a, T> Generator<T> for RecursiveGenerator<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let state = &self.state;
        let thread = std::thread::current().id();
        let is_outermost = {
            let mut frames = state.frames.lock();
            let frame = frames.entry(thread).or_insert(Frame {
                depth: 0,
                remaining: 0,
            });
            frame.depth += 1;
            frame.depth == 1
        };
        let _guard = FrameGuard {
            frames: &state.frames,
            thread,
        };
        tc.start_span(RECURSIVE_LABEL);
        if is_outermost {
            let budget = tc.generate_integer_i64(1, state.max_size);
            state.frames.lock().get_mut(&thread).unwrap().remaining = budget;
        }
        let budget_allows_expansion = {
            let mut frames = state.frames.lock();
            let frame = frames.get_mut(&thread).unwrap();
            frame.remaining -= 1;
            frame.remaining >= 1
        };
        let expand = budget_allows_expansion && tc.generate_boolean(BRANCH_PROBABILITY);
        let result = if expand {
            state.extended.get().unwrap().do_draw(tc)
        } else {
            state.base.do_draw(tc)
        };
        tc.stop_span(false);
        result
    }
}

/// Generate recursive structures — trees, nested expressions, and other
/// self-referential data — under a size budget, so that generated values
/// span the whole range from a single base value up to large structures
/// with around `max_size` generator nodes.
///
/// `base` generates the non-recursive leaves. `extend` is called once with
/// a handle to the recursive generator itself and returns the generator for
/// compound values; each draw from the handle produces a sub-structure.
/// `max_size` caps the size of one generated value, counted in draws from
/// the recursive generator (leaves and compound nodes together).
///
/// Every top-level draw first picks a target size uniformly from
/// `1..=max_size`, then expands nodes with high probability until that
/// budget is spent, so sizes are well spread rather than clustered: small
/// values stay common while structures near the full budget are generated
/// routinely. The budget is approximate — a compound node drawn just before
/// exhaustion still completes, so a value can overrun the target by the
/// arity of its last few nodes — and it also bounds the recursion depth, so
/// generation never overflows the stack. Failures shrink toward a single
/// base value.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self as gs, Generator};
///
/// #[derive(Debug, Clone)]
/// enum Expr {
///     Value(i64),
///     Add(Box<Expr>, Box<Expr>),
/// }
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let exprs = gs::recursive(
///         gs::integers::<i64>().map(Expr::Value),
///         |inner| {
///             hegel::tuples!(inner.clone(), inner)
///                 .map(|(l, r)| Expr::Add(Box::new(l), Box::new(r)))
///         },
///         100,
///     );
///     let e = tc.draw(exprs);
/// }
/// ```
///
/// # Panics
///
/// Panics if `max_size` is zero.
pub fn recursive<'a, T, G, F>(
    base: impl Generator<T> + Send + Sync + 'a,
    extend: F,
    max_size: u64,
) -> RecursiveGenerator<'a, T>
where
    G: Generator<T> + Send + Sync + 'a,
    F: FnOnce(RecursiveGenerator<'a, T>) -> G,
{
    if max_size == 0 {
        invalid_argument!("recursive: max_size must be at least 1");
    }
    let state = Arc::new(RecursiveState {
        base: base.boxed(),
        extended: OnceLock::new(),
        max_size: i64::try_from(max_size).unwrap_or(i64::MAX),
        frames: Mutex::new(HashMap::new()),
    });
    let generator = RecursiveGenerator {
        state: Arc::clone(&state),
    };
    let extended = extend(generator.clone());
    let _ = state.extended.set(extended.boxed());
    generator
}
