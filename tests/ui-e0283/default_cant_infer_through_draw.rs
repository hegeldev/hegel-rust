// `gs::default()` cannot infer the generated type through `tc.draw(...)`:
// the generator's type parameter needs an annotation. (The draw result is
// deliberately left unannotated too — annotating it changes nothing about
// whether this compiles, but makes the compiler enumerate candidate
// `Generator` impls in the diagnostic, and that list is not stable across
// toolchains.)

use hegel::generators as gs;

fn _check(tc: &hegel::TestCase) {
    let _ = tc.draw(gs::default());
}

fn main() {}
