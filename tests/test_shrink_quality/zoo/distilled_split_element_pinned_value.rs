//! Distilled from debian-changelog/9: a dead element of a count-driven list whose separator is
//! drawn much later, with a value the property *pins* sitting between the two.
//!
//! `n ∈ [0, 3]` items `(kind ∈ [0, 2], value ∈ [0, 9])`; a coin `c ∈ [0, 14]`, drawn either only
//! when a kind-`0` item exists or always; a header `h ∈ [0, 100]`; `n` separators; the payload
//! `x ∈ [0, 100]`. The property fails iff `x ≥ 50` and, per variant, the coin or the header is
//! pinned non-minimal. `distilled_split_element_draws` (nothing pinned) passes by a trick:
//! `delete_chunks` drops the window `[item, h]` and the separator stands in for the dead header.
//! Anything pinned between defeats it, because the stand-in has to be a same-kind, in-range draw
//! whose recorded value fails the pin (the engine consumes a prefix value only if it is an
//! `Integer` that validates in the draw's range, else it samples afresh). The controls bound the
//! stall: a conditional coin goes *with* its item; separators whose range covers the pinned
//! value stand in for it; boolean or out-of-range separators make the header a fresh draw.
//! Shortlex ideals: `([], None, 1, [], 50)` pinned header, `([], Some(1), 0, [], 50)` pinned
//! coin, `([], None, 2, [], 50)` header at two. A human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<(u8, u8)>, Option<u8>, i64, Vec<u8>, i64);

fn small(max: u8) -> gs::IntegerGenerator<u8> {
    gs::integers::<u8>().max_value(max)
}

fn field() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(100)
}

fn items(tc: &TestCase) -> Vec<(u8, u8)> {
    let n: usize = tc.draw_silent(gs::integers::<usize>().max_value(3));
    (0..n)
        .map(|_| (tc.draw_silent(small(2)), tc.draw_silent(small(9))))
        .collect()
}

fn conditional_coin(tc: &TestCase, items: &[(u8, u8)]) -> Option<u8> {
    items
        .iter()
        .any(|&(k, _)| k == 0)
        .then(|| tc.draw_silent(small(14)))
}

fn draw_with_sep(tc: &TestCase, conditional: bool, sep_min: u8) -> Draws {
    let items = items(tc);
    let coin = if conditional {
        conditional_coin(tc, &items)
    } else {
        Some(tc.draw_silent(small(14)))
    };
    let h = tc.draw_silent(field());
    let seps: Vec<u8> = (0..items.len())
        .map(|_| tc.draw_silent(small(sep_min + 2).min_value(sep_min)))
        .collect();
    let x = tc.draw_silent(field());
    (items, coin, h, seps, x)
}

fn draw_conditional(tc: &TestCase) -> Draws {
    draw_with_sep(tc, true, 0)
}

fn draw_unconditional(tc: &TestCase) -> Draws {
    draw_with_sep(tc, false, 0)
}

fn draw_conditional_sep_from_one(tc: &TestCase) -> Draws {
    draw_with_sep(tc, true, 1)
}

fn draw_conditional_bool_seps(tc: &TestCase) -> Draws {
    let items = items(tc);
    let coin = conditional_coin(tc, &items);
    let h = tc.draw_silent(field());
    let seps: Vec<u8> = (0..items.len())
        .map(|_| u8::from(tc.draw_silent(gs::booleans())))
        .collect();
    let x = tc.draw_silent(field());
    (items, coin, h, seps, x)
}

fn draw_conditional_ranged_header(tc: &TestCase) -> Draws {
    let items = items(tc);
    let coin = conditional_coin(tc, &items);
    let h = tc.draw_silent(gs::integers::<i64>().min_value(5).max_value(100));
    let seps: Vec<u8> = (0..items.len()).map(|_| tc.draw_silent(small(2))).collect();
    let x = tc.draw_silent(field());
    (items, coin, h, seps, x)
}

fn payload_and_coin_off((_items, coin, _h, _seps, x): &Draws) -> bool {
    *x >= 50 && coin.is_none_or(|c| c != 0)
}

fn payload_and_header((_items, _coin, h, _seps, x): &Draws) -> bool {
    *x >= 50 && *h != 0
}

fn payload_and_header_two((_items, _coin, h, _seps, x): &Draws) -> bool {
    *x >= 50 && *h >= 2
}

fn payload_only((_items, _coin, _h, _seps, x): &Draws) -> bool {
    *x >= 50
}

fn conditional_ideal() -> Draws {
    (vec![], None, 0, vec![], 50)
}

fn pinned_header_ideal() -> Draws {
    (vec![], None, 1, vec![], 50)
}

fn pinned_coin_ideal() -> Draws {
    (vec![], Some(1), 0, vec![], 50)
}

#[test]
fn the_ideals_fail_and_are_smallest() {
    assert!(payload_and_coin_off(&conditional_ideal()));
    assert!(!payload_and_coin_off(&(vec![], None, 0, vec![], 49)));
    assert!(!payload_and_coin_off(&(
        vec![(0, 0)],
        Some(0),
        0,
        vec![0],
        50
    )));
    assert!(payload_and_coin_off(&(
        vec![(0, 0)],
        Some(1),
        0,
        vec![0],
        50
    )));
    assert!(payload_and_header(&pinned_header_ideal()));
    assert!(!payload_and_header(&conditional_ideal()));
    assert!(payload_and_coin_off(&pinned_coin_ideal()));
    assert!(!payload_and_coin_off(&(vec![], Some(0), 0, vec![], 50)));
    assert!(payload_only(&conditional_ideal()));
}

#[test]
#[ignore = "shrinker: no pass deletes two separated regions at once"]
fn the_item_is_deleted_past_a_pinned_header() {
    assert_shrinks_to(
        &pinned_header_ideal(),
        30,
        200,
        draw_conditional,
        payload_and_header,
    );
}

#[test]
#[ignore = "shrinker: no pass deletes two separated regions at once"]
fn the_item_is_deleted_past_a_pinned_coin() {
    assert_shrinks_to(
        &pinned_coin_ideal(),
        30,
        200,
        draw_unconditional,
        payload_and_coin_off,
    );
}

#[test]
#[ignore = "shrinker: no pass deletes two separated regions at once"]
fn the_item_is_deleted_past_a_header_no_separator_can_stand_in_for() {
    assert_shrinks_to(
        &(vec![], None, 2, vec![], 50),
        30,
        200,
        draw_conditional_sep_from_one,
        payload_and_header_two,
    );
}

#[test]
fn control_the_item_is_deleted_when_a_separator_can_stand_in_for_the_pinned_header() {
    assert_shrinks_to(
        &pinned_header_ideal(),
        30,
        200,
        draw_conditional_sep_from_one,
        payload_and_header,
    );
}

#[test]
fn control_boolean_separators_with_a_dead_header() {
    assert_shrinks_to(
        &conditional_ideal(),
        30,
        200,
        draw_conditional_bool_seps,
        payload_only,
    );
}

#[test]
fn control_boolean_separators_with_a_pinned_header() {
    assert_shrinks_to(
        &pinned_header_ideal(),
        30,
        200,
        draw_conditional_bool_seps,
        payload_and_header,
    );
}

#[test]
fn control_out_of_range_separators_with_a_range_pinned_header() {
    assert_shrinks_to(
        &(vec![], None, 5, vec![], 50),
        30,
        200,
        draw_conditional_ranged_header,
        payload_only,
    );
}

#[test]
fn control_the_item_and_its_conditional_coin_are_deleted() {
    assert_shrinks_to(
        &conditional_ideal(),
        30,
        200,
        draw_conditional,
        payload_and_coin_off,
    );
}

#[test]
fn control_the_item_and_an_unconstrained_conditional_coin_are_deleted() {
    assert_shrinks_to(
        &conditional_ideal(),
        30,
        200,
        draw_conditional,
        payload_only,
    );
}
