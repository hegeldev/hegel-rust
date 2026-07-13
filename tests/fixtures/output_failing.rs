//! Fixture binary: a failing `hegel::hegel` run whose stderr layout —
//! draw line, panic diagnostic, and (with `RUST_BACKTRACE`) the backtrace
//! frames of this crate and hegel's runner — the output tests assert on.

use hegel::generators as gs;

fn main() {
    hegel::hegel(|tc| {
        let x = tc.draw(gs::integers::<i32>());
        panic!("intentional failure: {}", x);
    });
}
