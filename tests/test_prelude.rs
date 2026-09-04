//! `hegel::prelude` covers the imports a typical test needs: the generator
//! traits (so combinator and boxing methods resolve), the derivable traits
//! with their derive macros, `TestCase`, and the `gs` alias.

use hegel::prelude::*;

#[derive(Debug, Clone, DefaultGenerator, PrettyPrintable)]
struct Config {
    threshold: u8,
}

fn silent_bools() -> impl Generator<bool> {
    struct Silent;
    impl Generator<bool> for Silent {
        fn do_draw(&self, tc: &TestCase) -> bool {
            tc.draw_silent(gs::booleans())
        }
    }
    Silent
}

#[hegel::test]
fn prelude_brings_generator_methods_and_derives_into_scope(tc: TestCase) {
    let n = tc.draw(gs::integers::<i32>().map(|n| n / 2).boxed());
    let config: Config = tc.draw(gs::default::<Config>());
    let flag = tc.draw(silent_bools().print_as_debug().boxed_printable());
    let _ = (n, config.threshold, flag);
}

#[hegel::test]
fn prelude_brings_pretty_printable_into_scope(tc: TestCase) {
    let config: Config = tc.draw(gs::default::<Config>());
    let mut doc = hegel::Document::new();
    config.pretty_print(doc.printer());
    assert!(doc.finish().starts_with("Config {"));
}
