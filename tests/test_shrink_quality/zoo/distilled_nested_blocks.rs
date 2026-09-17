//! Distilled: numbered `Begin(k)` / `End(k)` blocks around a payload.
//!
//! `tokens = vecs((kind ∈ [0, 2], k ∈ [0, 3])).max_size(10)`: kind 0 is `Begin(k)`, 1 is `End(k)`,
//! 2 is `Payload(k)`. The property fails iff the list is well-formed (every `End(k)` closes the
//! innermost open `Begin(k)`, nothing stays open) and some non-zero payload sits inside a block.
//! Shortlex ideal `[Begin(0), Payload(1), End(0)]`. The dead enclosing blocks are deleted; the
//! surviving block's label can only be lowered together with its partner, and other nodes share
//! its value: at `2` the `Payload`'s kind, which has other bounds, and at `1` the `End`'s kind
//! and the payload value itself, which has the labels' own bounds — so `shrink_duplicates` has
//! to retry the group with a member left out, not just split by constraints.
//! `distilled_labels_with_tag` is the same shape in six draws. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Begin(u8),
    End(u8),
    Payload(u8),
}

fn draw(tc: &TestCase) -> Vec<Token> {
    let raw: Vec<(u8, u8)> = tc.draw_silent(
        gs::vecs(gs::tuples!(
            gs::integers::<u8>().max_value(2),
            gs::integers::<u8>().max_value(3)
        ))
        .max_size(10),
    );
    raw.into_iter()
        .map(|(kind, k)| match kind {
            0 => Token::Begin(k),
            1 => Token::End(k),
            _ => Token::Payload(k),
        })
        .collect()
}

fn well_formed_with_payload_inside(tokens: &[Token]) -> bool {
    let mut open: Vec<u8> = Vec::new();
    let mut inside = false;
    for t in tokens {
        match t {
            Token::Begin(k) => open.push(*k),
            Token::End(k) => {
                if open.pop() != Some(*k) {
                    return false;
                }
            }
            Token::Payload(v) => {
                if *v != 0 && !open.is_empty() {
                    inside = true;
                }
            }
        }
    }
    open.is_empty() && inside
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    use Token::*;
    assert!(well_formed_with_payload_inside(&[
        Begin(0),
        Payload(1),
        End(0)
    ]));
    assert!(!well_formed_with_payload_inside(&[Payload(1)]));
    assert!(!well_formed_with_payload_inside(&[Begin(0), Payload(1)]));
    assert!(!well_formed_with_payload_inside(&[
        Begin(0),
        Payload(0),
        End(0)
    ]));
    assert!(!well_formed_with_payload_inside(&[
        Begin(0),
        Payload(1),
        End(1)
    ]));
    assert!(well_formed_with_payload_inside(&[
        Begin(0),
        Begin(1),
        Payload(1),
        End(1),
        End(0)
    ]));
    assert!(!well_formed_with_payload_inside(&[
        Begin(0),
        Payload(1),
        End(1),
        End(0)
    ]));
}

#[test]
fn surviving_block_label_is_lowered_to_zero() {
    assert_shrinks_to(
        &vec![Token::Begin(0), Token::Payload(1), Token::End(0)],
        30,
        2000,
        draw,
        |tokens| well_formed_with_payload_inside(tokens),
    );
}
