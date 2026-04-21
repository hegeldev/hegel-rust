//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_core.py
//!
//! Individually-skipped tests:
//! - `test_stops_after_max_examples_if_satisfying`,
//!   `test_stops_after_ten_times_max_examples_if_not_satisfying` — both drive
//!   `find(strategy, predicate)` and assert `count == max_examples` /
//!   `count <= 10*max_examples` against the predicate-call counter inside
//!   `find()`. hegel-rust has no `find()` public API, and `Hegel::new(...).run()`
//!   re-enters the test function for span-mutation attempts (up to 5 per valid
//!   case in native), so the predicate-call shape Python's `find()` pins down
//!   isn't reproducible through the Rust public surface.
//! - `test_is_not_normally_default`, `test_settings_are_default_in_given` —
//!   both inspect `settings.default`, a Python module-level mutable global; no
//!   Rust counterpart (hegel-rust settings are constructed per-test via
//!   `Settings::new()`).
//! - `test_pytest_skip_skips_shrinking` — relies on `pytest.skip()` inside a
//!   `@given` body to abort shrinking; hegel-rust has no per-test "skip aborts
//!   shrinking" mechanism on its public API.
//! - `test_no_such_example` — uses `find(..., database_key=b"...")` and asserts
//!   `NoSuchExample`; both are `find()`-API surface (see above).
//! - `test_validates_strategies_for_test_method` — uses `st.lists(st.nothing())`;
//!   hegel-rust has no `gs::nothing()` (same gap as
//!   `test_given_error_conditions.py::test_raises_unsatisfiable_if_passed_explicit_nothing`).
//! - `test_non_executed_tests_raise_skipped` — exercises
//!   `Phase.target/shrink/explain/explicit/reuse` settings; hegel-rust has no
//!   public `Phase`/`phases` setting.

use crate::common::utils::minimal;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

#[test]
fn test_given_shrinks_pytest_helper_errors() {
    let value = minimal(gs::integers::<i64>(), |x: &i64| *x > 100);
    assert_eq!(value, 101);
}

#[test]
fn test_can_find_with_db_eq_none() {
    Hegel::new(|tc| {
        let _: i64 = tc.draw(gs::integers::<i64>());
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

// test_characters_codec parametrize rows: each row drives one assertion that
// the codec / max_codepoint / categories / exclude_categories constraint is
// honoured by every drawn character. The Python original asserts the full
// codec round-trip (`example.encode(codec).decode(codec) == example`); Rust
// `char` is always a Unicode scalar, so for "ascii" the round-trip reduces to
// `c.is_ascii()` and for "utf-8" it is trivially true. The Unicode-category
// rows need `unicodedata::general_category` and are native-gated.

#[test]
fn test_characters_codec_ascii_unbounded() {
    Hegel::new(|tc| {
        let c: char = tc.draw(gs::characters().codec("ascii"));
        assert!(c.is_ascii());
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_characters_codec_ascii_max_codepoint_128() {
    Hegel::new(|tc| {
        let c: char = tc.draw(gs::characters().codec("ascii").max_codepoint(128));
        assert!(c.is_ascii());
        assert!(c as u32 <= 128);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_characters_codec_ascii_max_codepoint_100() {
    Hegel::new(|tc| {
        let c: char = tc.draw(gs::characters().codec("ascii").max_codepoint(100));
        assert!(c.is_ascii());
        assert!(c as u32 <= 100);
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_characters_codec_utf8_unbounded() {
    Hegel::new(|tc| {
        let _: char = tc.draw(gs::characters().codec("utf-8"));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[test]
fn test_characters_codec_utf8_exclude_cs() {
    // Rust `char` already excludes the surrogate range by construction, so
    // exclude_categories=["Cs"] is a no-op for the round-trip property; we
    // still exercise the schema path to make sure it doesn't reject.
    Hegel::new(|tc| {
        let _: char = tc.draw(gs::characters().codec("utf-8").exclude_categories(&["Cs"]));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[cfg(feature = "native")]
fn category_of(c: char) -> &'static str {
    hegel::__native_test_internals::unicodedata::general_category(c as u32).as_str()
}

#[cfg(feature = "native")]
#[test]
fn test_characters_codec_utf8_exclude_n() {
    Hegel::new(|tc| {
        let c: char = tc.draw(gs::characters().codec("utf-8").exclude_categories(&["N"]));
        assert!(!category_of(c).starts_with('N'));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}

#[cfg(feature = "native")]
#[test]
fn test_characters_codec_utf8_categories_n() {
    Hegel::new(|tc| {
        let c: char = tc.draw(gs::characters().codec("utf-8").categories(&["N"]));
        assert!(category_of(c).starts_with('N'));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}
