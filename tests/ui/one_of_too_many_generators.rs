// `one_of!` supports up to 12 component generators; a thirteenth must
// produce a clear error pointing at the vec-based `one_of` function.
//
// (The `_used` binding keeps the `gs` import live — the compile_error!
// discards the macro arguments before they can use it — and doubles as
// filler keeping every line rustc renders in the diagnostic at a two-digit
// line number: toolchains disagree on how single-digit line numbers are
// aligned in a multi-digit gutter, and this golden must match both the
// MSRV and current compilers.)

use hegel::generators as gs;

fn _check(tc: &hegel::TestCase) {
    let _used = gs::just(0);
    let _ = tc.draw(hegel::one_of!(
        gs::just(0),
        gs::just(1),
        gs::just(2),
        gs::just(3),
        gs::just(4),
        gs::just(5),
        gs::just(6),
        gs::just(7),
        gs::just(8),
        gs::just(9),
        gs::just(10),
        gs::just(11),
        gs::just(12),
    ));
}

fn main() {}
