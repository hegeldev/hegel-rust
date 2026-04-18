//! `#[hegel::state_machine]` must copy any outer `#[cfg(...)]` attributes onto
//! the items it synthesises; otherwise the `compile_error!` below would fire
//! during macro expansion even though the cfg predicate is inactive.
//! see https://github.com/hegeldev/hegel-rust/issues/151

#![allow(unexpected_cfgs)]

use hegel::TestCase;

struct A {
    count: u32,
}

#[hegel::state_machine]
impl A {
    #[rule]
    fn increment(&mut self, _tc: TestCase) {
        self.count += 1;
    }

    #[cfg(nonexistent_config)]
    #[rule]
    fn f1(&mut self, _tc: TestCase) {
        compile_error!("should be compiled out");
    }

    #[cfg(nonexistent_config)]
    #[invariant]
    fn f2(&mut self, _tc: TestCase) {
        compile_error!("should be compiled out");
    }
}

fn main() {
    let _a = A { count: 0 };
}
