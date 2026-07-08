use super::generators::draw_and_print_value;
use super::{Generator, PrintableGenerator, TestCase};
use crate::pretty::{PrettyPrintable, PrettyPrinter};

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

impl<T: Clone + Send + Sync + PrettyPrintable> PrintableGenerator<T> for JustGenerator<T> {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> T {
        draw_and_print_value(self, tc, printer)
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

impl PrintableGenerator<bool> for BoolGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> bool {
        draw_and_print_value(self, tc, printer)
    }
}

/// Generate boolean values.
pub fn booleans() -> BoolGenerator {
    BoolGenerator
}
