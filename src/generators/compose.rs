use super::{Generator, TestCase};
use std::marker::PhantomData;

/// A generator built from imperative code. Created by [`compose!`](crate::compose).
#[doc(hidden)]
pub struct ComposedGenerator<T, F> {
    f: F,
    _phantom: PhantomData<fn() -> T>,
}

impl<T, F> ComposedGenerator<T, F>
where
    F: Fn(TestCase) -> T,
{
    /// Create a composed generator from a closure that receives a [`TestCase`].
    pub fn new(f: F) -> Self {
        ComposedGenerator {
            f,
            _phantom: PhantomData,
        }
    }
}

impl<T, F> Generator<T> for ComposedGenerator<T, F>
where
    F: Fn(TestCase) -> T + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        (self.f)(tc.clone())
    }
}

/// Compile-time FNV-1a hash of a byte slice, producing a u64 label.
#[doc(hidden)]
// nocov start
pub const fn fnv1a_hash(bytes: &[u8]) -> u64 {
    // nocov end
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    // nocov start
    let mut hash = FNV_OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash
    // nocov end
}

/// Create a generator from imperative code that draws from other generators.
///
/// This is analogous to Hypothesis's `@composite` decorator. The closure
/// receives a `TestCase` parameter. Use `tc.draw()` to draw values from
/// other generators within the compose block.
///
/// # Example
///
/// ```no_run
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let value = tc.draw(hegel::compose!(|tc| {
///         let x = tc.draw(gs::integers::<i32>().min_value(0).max_value(10));
///         let y = tc.draw(gs::integers::<i32>().min_value(x).max_value(100));
///         (x, y)
///     }));
/// }
/// ```
#[macro_export]
macro_rules! compose {
    (|$tc:ident| { $($body:tt)* }) => {{
        const LABEL: u64 = $crate::generators::fnv1a_hash(stringify!($($body)*).as_bytes());
        $crate::generators::ComposedGenerator::new(move |$tc: $crate::TestCase| {
            $tc.start_span(LABEL);
            let __result = { $($body)* };
            $tc.stop_span(false);
            __result
        })
    }};
}
