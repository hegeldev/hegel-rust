// A `one_of!` whose components are not all printable is a valid generator,
// but only for `draw_silent`: passing it to `tc.draw(...)` must fail with
// the `PrintableGenerator` bound, rooted at the missing `PrettyPrintable`
// implementation on the produced type. (`OsString` stands in for any
// non-printable type; a locally-defined struct would work the same but its
// diagnostic rendering differs across toolchains.)

use hegel::generators as gs;
use std::ffi::OsString;

fn _check(tc: &hegel::TestCase) {
    let _ = tc.draw(hegel::one_of!(
        gs::just(OsString::new()),
        gs::just(OsString::new()),
    ));
}

fn main() {}
