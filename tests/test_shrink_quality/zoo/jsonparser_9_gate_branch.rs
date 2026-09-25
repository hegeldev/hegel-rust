//! From hegel-zoo `go/jsonparser`, bug jsonparser/9, test `TestHegelDeleteOfAKeyThatLooksLikeAnIndex`.
//!
//! The test builds an object key starting with `[`: normally `"["` plus up to three alphabet
//! characters, but with a 30% chance — `n(1, 100) <= 30`, an integer draw used as a coin — it
//! uses `"[<digit>]"` instead, at the cost of one extra draw. Every such key triggers the bug,
//! so the shortlex ideal is the key `"["`: gate 31 (anything above 30), no characters, no digit.
//! Hegel's first test case is the all-minimal sequence, gate 1 / digit 0 = `"[0]"`, and getting
//! from there to `"["` needs the gate raised from 1 to 31 and the digit draw deleted in one step.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

const KEY_ALPHABET: &[char] = &[
    'a', 'b', 'z', 'A', 'Z', '0', '9', ' ', '_', '-', '/', ':', ',', ';', '=', '!', '?', '\'', '#',
    '@', '$', '%', '&', '*', '+', '<', '>', '(', ')', '{', '}', '|', '^', '~', '`', 'é', '中',
    '日', '😀',
];

fn bracket_key(tc: &TestCase) -> String {
    let idx: Vec<usize> = tc.draw_silent(
        gs::vecs(gs::integers::<usize>().max_value(KEY_ALPHABET.len() - 1)).max_size(3),
    );
    let mut key: String = "[".to_string();
    key.extend(idx.iter().map(|&i| KEY_ALPHABET[i]));
    let gate = tc.draw_silent(gs::integers::<i64>().min_value(1).max_value(100));
    if gate <= 30 {
        let d = tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(3));
        key = format!("[{d}]");
    }
    key
}

#[test]
fn larger_gate_value_selects_the_shorter_branch() {
    assert_shrinks_to(&"[".to_string(), 20, 100, bracket_key, |_| true);
}
