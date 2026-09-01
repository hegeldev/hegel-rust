// `one_of!` supports up to 30 component generators; a thirty-first must
// produce a clear error pointing at the vec-based `one_of` function and
// the `impl_one_of!` macro.
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
    tc.draw(hegel::one_of!(
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
        gs::just(13),
        gs::just(14),
        gs::just(15),
        gs::just(16),
        gs::just(17),
        gs::just(18),
        gs::just(19),
        gs::just(20),
        gs::just(21),
        gs::just(22),
        gs::just(23),
        gs::just(24),
        gs::just(25),
        gs::just(26),
        gs::just(27),
        gs::just(28),
        gs::just(29),
        gs::just(30),
    ));
}

fn main() {}
