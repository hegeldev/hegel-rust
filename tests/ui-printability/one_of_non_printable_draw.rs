// A `one_of!` whose components are not all printable is a valid generator,
// but only for `draw_silent`: passing it to `tc.draw(...)` must fail with
// the `PrintableGenerator` bound, rooted at the missing `Debug`
// implementation on the produced type. (A locally-defined struct without
// `Debug` stands in for any non-printable type.)

use hegel::generators as gs;

#[derive(Clone)]
struct Opaque;

fn _check(tc: &hegel::TestCase) {
    let _ = tc.draw(hegel::one_of!(gs::just(Opaque), gs::just(Opaque)));
}

fn main() {}
