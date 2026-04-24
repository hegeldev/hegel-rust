//! Ported from hypothesis-python/tests/nocover/test_characters.py
//!
//! Individually-skipped tests:
//!
//! - `test_can_constrain_characters_to_codec` — parametrizes over
//!   Python's `encodings.aliases.aliases` dict (100+ codec names like
//!   `cp1252`, `shift_jis`, `koi8-r`) and asserts the generated string
//!   encodes via Python's `str.encode(codec)`. Both the codec-list source
//!   and the verification step are Python-stdlib integrations with no
//!   Rust counterpart; hegel-rust's `codec` support in `src/native/` is
//!   limited to `ascii` / `latin-1` / `utf-8`.

use crate::common::utils::assert_all_examples;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

const IDENTIFIER_CHARS: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_";

#[test]
fn test_large_blacklist() {
    assert_all_examples(
        gs::characters().exclude_characters(IDENTIFIER_CHARS),
        |c: &char| !IDENTIFIER_CHARS.contains(*c),
    );
}

#[test]
fn test_arbitrary_blacklist() {
    Hegel::new(|tc| {
        let blacklist: String = tc.draw(gs::text().max_codepoint(1000).min_size(1));
        let ords: Vec<u32> = blacklist.chars().map(|c| c as u32).collect();
        let min_cp = ords.iter().min().copied().unwrap().saturating_sub(1);
        let max_cp = ords.iter().max().copied().unwrap() + 1;
        let c: char = tc.draw(
            gs::characters()
                .exclude_characters(&blacklist)
                .min_codepoint(min_cp)
                .max_codepoint(max_cp),
        );
        assert!(!blacklist.contains(c));
    })
    .settings(Settings::new().test_cases(100).database(None))
    .run();
}
