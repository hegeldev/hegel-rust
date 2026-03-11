use super::{Generator, TestCaseData};
use std::marker::PhantomData;

pub struct ComposedGenerator<T, F> {
    f: F,
    _phantom: PhantomData<fn() -> T>,
}

impl<T, F> ComposedGenerator<T, F>
where
    F: Fn() -> T,
{
    pub fn new(f: F) -> Self {
        ComposedGenerator {
            f,
            _phantom: PhantomData,
        }
    }
}

impl<T, F> Generator<T> for ComposedGenerator<T, F>
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
/// #[hegel::test]
/// fn my_test() {
///     let value = hegel::draw(&hegel::compose!(|draw| {
///         let x = draw(&generators::integers::<i32>().min_value(0).max_value(10));
///         let y = draw(&generators::integers::<i32>().min_value(x).max_value(100));
///         (x, y)
///     }));
/// }
/// ```
#[macro_export]
macro_rules! compose {
    (|$draw:ident| { $($body:tt)* }) => {{
        const LABEL: u64 = $crate::generators::fnv1a_hash(stringify!($($body)*).as_bytes());
        $crate::generators::ComposedGenerator::new(move || {
            let __data = $crate::generators::test_case_data().expect(
                "compose!() cannot be called outside of a Hegel test."
            );
            let __was_composite = __data.in_composite.get();
            __data.in_composite.set(true);
            __data.start_span(LABEL);
            let __result = {
                fn $draw<T>(gen: &impl $crate::generators::Generator<T>) -> T {
                    gen.do_draw($crate::generators::test_case_data().expect(
                        "compose!() cannot be called outside of a Hegel test."
                    ))
                }
                $($body)*
            };
            __data.stop_span(false);
            __data.in_composite.set(__was_composite);
            __result
        })
    }};
}
