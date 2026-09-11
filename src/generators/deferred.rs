use super::{
    BoxedGenerator, BoxedPrintableGenerator, Generator, PrintableGenerator, label_from_name,
};
use crate::pretty::PrettyPrinter;
use crate::test_case::TestCase;
use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

/// The label a deferred generator reports for itself from inside its own
/// label computation. A deferred definition can refer to itself (that is what
/// it is for), so computing its label by asking its components would recurse
/// forever; the reference back to the definition stands in with this
/// constant instead, as Hypothesis's `calculating` sentinel does.
const DEFERRED_LABEL: u64 = label_from_name("hegel.deferred");

/// What every handle from one [`deferred()`] definition shares: the
/// generator, once set, and its label, once computed.
struct Shared<B> {
    generator: OnceLock<B>,
    label: OnceLock<u64>,
}

impl<B> Shared<B> {
    fn new() -> Self {
        Shared {
            generator: OnceLock::new(),
            label: OnceLock::new(),
        }
    }
}

thread_local! {
    /// The definitions whose labels are being computed on this thread, by
    /// address, so a self-reference met on the way can be recognised.
    static CALCULATING_LABELS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Removes its definition from [`CALCULATING_LABELS`] when dropped, so a
/// panic partway through a label computation cannot leave it marked.
struct CalculatingLabel(usize);

impl Drop for CalculatingLabel {
    fn drop(&mut self) {
        CALCULATING_LABELS.with(|calculating| {
            let mut calculating = calculating.borrow_mut();
            let position = calculating.iter().rposition(|&key| key == self.0).unwrap();
            calculating.remove(position);
        });
    }
}

struct DeferredGenerator<B> {
    inner: Arc<Shared<B>>,
}

impl<B> DeferredGenerator<B> {
    fn get(&self) -> &B {
        self.inner
            .generator
            .get()
            .unwrap_or_else(|| panic!("DeferredGenerator has not been set"))
    }
}

impl<T, B: Generator<T> + Send + Sync> Generator<T> for DeferredGenerator<B> {
    fn label(&self) -> u64 {
        if let Some(&label) = self.inner.label.get() {
            return label;
        }
        let generator = self.get();
        let key = Arc::as_ptr(&self.inner) as usize;
        let in_progress = CALCULATING_LABELS.with(|calculating| {
            let mut calculating = calculating.borrow_mut();
            let in_progress = calculating.contains(&key);
            if !in_progress {
                calculating.push(key);
            }
            in_progress
        });
        if in_progress {
            return DEFERRED_LABEL;
        }
        let _guard = CalculatingLabel(key);
        let label = generator.label();
        *self.inner.label.get_or_init(|| label)
    }

    fn do_draw(&self, tc: &TestCase) -> T {
        self.get().do_draw(tc)
    }
}

impl<T, B: PrintableGenerator<T> + Send + Sync> PrintableGenerator<T> for DeferredGenerator<B> {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        self.get().do_draw_and_print(tc, printer)
    }
}

/// A deferred generator definition that can produce generator handles
/// before its implementation is known.
///
/// Created by [`deferred()`] (printable handles, the default) or
/// [`deferred_silent()`] (plain [`Generator`] handles, for implementations
/// that are not [`PrintableGenerator`]s — the second parameter is the boxed
/// handle type, mirroring [`OneOfGenerator`](super::OneOfGenerator)). Call
/// [`generator()`](Self::generator) to get handles that can be passed to
/// other generators, then call [`set()`](Self::set) to provide the actual
/// implementation. `set` consumes the definition, ensuring it can only be
/// called once.
///
/// # Panics
///
/// Drawing from a generator handle before [`set()`](Self::set) has been
/// called will panic.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self as gs, Generator};
///
/// #[derive(hegel::PrettyPrintable)]
/// enum Tree {
///     Leaf(i32),
///     Branch(Box<Tree>, Box<Tree>),
/// }
///
/// let tree = gs::deferred::<Tree>();
/// let leaf = gs::integers::<i32>().map(Tree::Leaf);
/// let branch = hegel::tuples!(tree.generator(), tree.generator())
///     .map(|(l, r)| Tree::Branch(Box::new(l), Box::new(r)));
/// tree.set(hegel::one_of!(leaf, branch));
/// ```
pub struct DeferredGeneratorDefinition<T, B = BoxedPrintableGenerator<'static, T>> {
    inner: Arc<Shared<B>>,
    _phantom: std::marker::PhantomData<fn(T)>,
}

impl<T: Send + Sync + 'static> DeferredGeneratorDefinition<T, BoxedPrintableGenerator<'static, T>> {
    /// Return a generator handle that will delegate to whatever is
    /// eventually passed to [`set()`](Self::set).
    ///
    /// Can be called multiple times to produce independent handles
    /// that all share the same underlying definition.
    pub fn generator(&self) -> BoxedPrintableGenerator<'static, T> {
        DeferredGenerator {
            inner: Arc::clone(&self.inner),
        }
        .boxed_printable()
    }

    /// Set the implementation for this deferred generator.
    ///
    /// All handles previously returned by [`generator()`](Self::generator)
    /// will delegate to the provided generator. Consumes the definition,
    /// so it can only be called once.
    ///
    /// # Panics
    ///
    /// Drawing from a handle before `set` is called will panic.
    pub fn set(self, generator: impl PrintableGenerator<T> + Send + Sync + 'static) {
        let _ = self.inner.generator.set(generator.boxed_printable());
    }
}

impl<T: Send + Sync + 'static> DeferredGeneratorDefinition<T, BoxedGenerator<'static, T>> {
    /// Return a generator handle that will delegate to whatever is
    /// eventually passed to [`set()`](Self::set).
    ///
    /// Can be called multiple times to produce independent handles
    /// that all share the same underlying definition.
    pub fn generator(&self) -> BoxedGenerator<'static, T> {
        DeferredGenerator {
            inner: Arc::clone(&self.inner),
        }
        .boxed()
    }

    /// Set the implementation for this deferred generator, which — unlike
    /// [`deferred()`]'s `set` — may be any plain [`Generator`].
    ///
    /// All handles previously returned by [`generator()`](Self::generator)
    /// will delegate to the provided generator. Consumes the definition,
    /// so it can only be called once.
    ///
    /// # Panics
    ///
    /// Drawing from a handle before `set` is called will panic.
    pub fn set(self, generator: impl Generator<T> + Send + Sync + 'static) {
        let _ = self.inner.generator.set(generator.boxed());
    }
}

/// Create a deferred generator definition for forward references.
///
/// Returns a [`DeferredGeneratorDefinition`] that can produce generator
/// handles before the implementation is known. This enables self-recursive
/// and mutually recursive generator definitions.
///
/// The handles are [`PrintableGenerator`]s, so the implementation passed to
/// `set` must be one too; for a recursive generator that cannot print, use
/// [`deferred_silent()`].
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self as gs, Generator};
///
/// #[derive(hegel::PrettyPrintable)]
/// enum Tree {
///     Leaf(i32),
///     Branch(Box<Tree>, Box<Tree>),
/// }
///
/// let tree = gs::deferred::<Tree>();
/// let leaf = gs::integers::<i32>().map(Tree::Leaf);
/// let branch = hegel::tuples!(tree.generator(), tree.generator())
///     .map(|(l, r)| Tree::Branch(Box::new(l), Box::new(r)));
/// tree.set(hegel::one_of!(leaf, branch));
/// ```
pub fn deferred<T>() -> DeferredGeneratorDefinition<T> {
    DeferredGeneratorDefinition {
        inner: Arc::new(Shared::new()),
        _phantom: std::marker::PhantomData,
    }
}

/// Create a deferred generator definition whose handles are plain
/// [`Generator`]s.
///
/// Like [`deferred()`], but `set` accepts any [`Generator`] — no
/// printability required. The handles are [`BoxedGenerator`]s, so they
/// follow the usual boxing rules (see [`Generator::boxed`]): drawable with
/// [`draw`](crate::TestCase::draw) when `T` is
/// [`PrettyPrintable`](crate::PrettyPrintable), and otherwise with
/// [`draw_silent`](crate::TestCase::draw_silent) or via
/// [`print_as_debug`](Generator::print_as_debug) or
/// [`print_with`](Generator::print_with).
pub fn deferred_silent<T>() -> DeferredGeneratorDefinition<T, BoxedGenerator<'static, T>> {
    DeferredGeneratorDefinition {
        inner: Arc::new(Shared::new()),
        _phantom: std::marker::PhantomData,
    }
}
