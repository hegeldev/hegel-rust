//! Control, distilled from textwrap/1's `wrappable_text`: a counted loop of tokens of different
//! widths, the payload being the *last* token.
//!
//! `n ∈ [0, 12]`, then `n` tokens, each `kind ∈ {0, 1}`: `Word` draws one more integer, `Spaces`
//! three. The property fails iff the last token is a `Spaces`. Shortlex ideal
//! `[Spaces(0, 0, 0)]`. Deleting a `Word` in front of the `Spaces` needs `n − 1` at the same
//! time, and the realised deficit `bind_deletion` sees is the width of the token that fell off
//! the end; a bigger drop of `n` makes the deficit span several tokens and the window aligns. A
//! human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(i64),
    Spaces(i64, i64, i64),
}

fn small(tc: &TestCase) -> i64 {
    tc.draw_silent(gs::integers::<i64>().min_value(0).max_value(100))
}

fn draw(tc: &TestCase) -> Vec<Token> {
    let n = tc.draw_silent(gs::integers::<usize>().max_value(12));
    (0..n)
        .map(|_| {
            let kind = tc.draw_silent(gs::integers::<u8>().max_value(1));
            if kind == 0 {
                Token::Word(small(tc))
            } else {
                Token::Spaces(small(tc), small(tc), small(tc))
            }
        })
        .collect()
}

fn ends_with_spaces(tokens: &[Token]) -> bool {
    matches!(tokens.last(), Some(Token::Spaces(..)))
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(ends_with_spaces(&[Token::Spaces(0, 0, 0)]));
    assert!(!ends_with_spaces(&[Token::Word(0)]));
    assert!(!ends_with_spaces(&[]));
}

#[test]
fn control_words_in_front_of_the_last_token_are_deleted() {
    assert_shrinks_to(&vec![Token::Spaces(0, 0, 0)], 30, 100, draw, |tokens| {
        ends_with_spaces(tokens)
    });
}
