use super::{Generate, TestCaseData};
use std::marker::PhantomData;

/// A generator created from imperative code that draws from other generators.
///
/// Use the `compose!` macro to create instances of this type.
///
/// `ComposedGenerator` wraps a closure that produces values by composing
/// multiple generator calls together. It never has a basic form (returns `None` from `as_basic()`),
/// since the composition is imperative and cannot be described as a single schema.
pub struct ComposedGenerator<T, F> {
    f: F,
    _phantom: PhantomData<fn() -> T>,
}

impl<T, F> ComposedGenerator<T, F>
where
    F: Fn() -> T,
{
    /// Create a new `ComposedGenerator` from a closure.
    ///
    /// Prefer using the `compose!` macro instead, which automatically
    /// wraps the body in a labeled span for better shrinking.
    pub fn new(f: F) -> Self {
        ComposedGenerator {
            f,
            _phantom: PhantomData,
        }
    }
}

impl<T, F> Generate<T> for ComposedGenerator<T, F>
where
    F: Fn() -> T + Send + Sync,
{
    fn do_draw(&self, _data: &TestCaseData) -> T {
        (self.f)()
    }
}

/// Compile-time FNV-1a hash of a byte slice, producing a u64 label.
pub const fn fnv1a_hash(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash
}

/// Create a generator from imperative code that draws from other generators.
///
/// This is analogous to Hypothesis's `@composite` decorator. The closure
/// receives a `draw` parameter that should be used to draw values from
/// other generators.
///
/// # Example
///
/// ```no_run
/// use hegel::generators;
///
/// # hegel::hegel(|| {
/// let value = hegel::draw(&hegel::compose!(|draw| {
///     let x = draw(&generators::integers::<i32>().with_min(0).with_max(10));
///     let y = draw(&generators::integers::<i32>().with_min(x).with_max(100));
///     (x, y)
/// }));
/// # });
/// ```
#[macro_export]
macro_rules! compose {
    (|$draw:ident| { $($body:tt)* }) => {{
        const LABEL: u64 = $crate::generators::fnv1a_hash(stringify!($($body)*).as_bytes());
        $crate::generators::ComposedGenerator::new(move || {
            let __data = $crate::generators::test_case_data().expect(
                "compose!() cannot be called outside of a Hegel test."
            );
            let __was_composite = __data.in_composite();
            __data.set_in_composite(true);
            let __result = __data.span_group(LABEL, || {
                fn $draw<T>(gen: &impl $crate::generators::Generate<T>) -> T {
                    gen.do_draw($crate::generators::test_case_data().expect(
                        "compose!() cannot be called outside of a Hegel test."
                    ))
                }
                $($body)*
            });
            __data.set_in_composite(__was_composite);
            __result
        })
    }};
}
