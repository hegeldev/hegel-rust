use super::{BoxedGenerator, BoxedPrintableGenerator, Generator, PrintableGenerator};
use crate::pretty::PrettyPrinter;
use crate::test_case::TestCase;
use std::sync::{Arc, OnceLock};

struct DeferredGenerator<B> {
    inner: Arc<OnceLock<B>>,
}

impl<B> DeferredGenerator<B> {
    fn get(&self) -> &B {
        self.inner
            .get()
            .unwrap_or_else(|| panic!("DeferredGenerator has not been set"))
    }
}

impl<T, B: Generator<T> + Send + Sync> Generator<T> for DeferredGenerator<B> {
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
    inner: Arc<OnceLock<B>>,
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
        let _ = self.inner.set(generator.boxed_printable());
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
        let _ = self.inner.set(generator.boxed());
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
        inner: Arc::new(OnceLock::new()),
        _phantom: std::marker::PhantomData,
    }
}

/// Create a deferred generator definition whose handles are plain
/// [`Generator`]s.
///
/// Like [`deferred()`], but `set` accepts any [`Generator`] — no
/// printability required — and the handles can only be drawn with
/// [`draw_silent`](crate::TestCase::draw_silent) (or made printable with
/// [`print_as_debug`](Generator::print_as_debug),
/// [`print_as_value`](Generator::print_as_value), or
/// [`print_with`](Generator::print_with)).
pub fn deferred_silent<T>() -> DeferredGeneratorDefinition<T, BoxedGenerator<'static, T>> {
    DeferredGeneratorDefinition {
        inner: Arc::new(OnceLock::new()),
        _phantom: std::marker::PhantomData,
    }
}
