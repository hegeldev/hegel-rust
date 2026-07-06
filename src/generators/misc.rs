use super::{Generator, TestCase};

/// Generate the unit value `()`.
// nocov start
pub fn unit() -> JustGenerator<()> {
    just(())
    // nocov end
}

/// Generator that always produces the same value. Created by [`just()`].
pub struct JustGenerator<T> {
    value: T,
}

impl<T: Clone + Send + Sync> Generator<T> for JustGenerator<T> {
    fn do_draw(&self, _tc: &TestCase) -> T {
        self.value.clone()
    }
}

/// Generate a constant value.
pub fn just<T: Clone + Send + Sync>(value: T) -> JustGenerator<T> {
    JustGenerator { value }
}

/// Generator for boolean values. Created by [`booleans()`].
pub struct BoolGenerator;

impl Generator<bool> for BoolGenerator {
    fn do_draw(&self, tc: &TestCase) -> bool {
        tc.generate_boolean(0.5)
    }
}

/// Generate boolean values.
pub fn booleans() -> BoolGenerator {
    BoolGenerator
}
