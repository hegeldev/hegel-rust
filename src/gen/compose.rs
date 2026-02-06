use super::Generate;
use ciborium::Value;

/// A generator created from imperative code that calls `.generate()` on other generators.
///
/// Use the [`compose!`] macro to create instances of this type.
///
/// `ComposedGenerator` wraps a closure that produces values by composing
/// multiple generator calls together. It has no schema (returns `None`),
/// since the composition is imperative and cannot be described as a single schema.
pub struct ComposedGenerator<T, F> {
    f: F,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, F> ComposedGenerator<T, F>
where
    F: Fn() -> T,
{
    /// Create a new `ComposedGenerator` from a closure.
    ///
    /// Prefer using the [`compose!`] macro instead, which automatically
    /// wraps the body in a labeled span for better shrinking.
    pub fn new(f: F) -> Self {
        ComposedGenerator {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T, F> Generate<T> for ComposedGenerator<T, F>
where
    F: Fn() -> T + Send + Sync,
{
    fn generate(&self) -> T {
        (self.f)()
    }

    fn schema(&self) -> Option<Value> {
        None
    }
}

// Safety: ComposedGenerator is Send+Sync if F is Send+Sync
unsafe impl<T, F: Send> Send for ComposedGenerator<T, F> {}
unsafe impl<T, F: Sync> Sync for ComposedGenerator<T, F> {}

/// Create a generator from imperative code that calls `.generate()` on other generators.
///
/// This is analogous to Hypothesis's `@composite` decorator. The body can call
/// `.generate()` on any generators and combine the results in arbitrary ways.
///
/// # Forms
///
/// ```no_run
/// use hegel::gen::{self, Generate};
///
/// // Default label (COMPOSE)
/// let gen = hegel::compose!({
///     let x = gen::integers::<i32>().with_min(0).with_max(10).generate();
///     let y = gen::integers::<i32>().with_min(x).with_max(100).generate();
///     (x, y)
/// });
///
/// // Custom label (evaluated per generate() call)
/// let gen = hegel::compose!(label: 42, {
///     let x = gen::integers::<i32>().generate();
///     x * 2
/// });
/// ```
///
/// # Shrinking
///
/// The body is wrapped in a labeled span, which helps the testing engine
/// understand the structure of generated data and improve shrinking.
/// The label expression and body are both evaluated on each `generate()` call.
#[macro_export]
macro_rules! compose {
    ({ $($body:tt)* }) => {
        $crate::gen::ComposedGenerator::new(move || {
            $crate::gen::group($crate::gen::labels::COMPOSE, || { $($body)* })
        })
    };
    (label: $label:expr, { $($body:tt)* }) => {
        $crate::gen::ComposedGenerator::new(move || {
            $crate::gen::group($label, || { $($body)* })
        })
    };
}
