#![allow(unused)]

use hegel::generators;
use hegel::stateful::{self, NamedRule, NamedInvariant, StateMachine};
use hegel::note;
use hegel::draw;
use std::cmp::{min, max};

struct DieHardState {
    small: i32,
    big: i32,
}

fn fill_small(s: &mut DieHardState) {
    s.small = 3;
}

fn fill_big(s: &mut DieHardState) {
    s.big = 5;
}

fn empty_small(s: &mut DieHardState) {
    s.small = 0;
}

fn empty_big(s: &mut DieHardState) {
    s.big = 0;
}

fn pour_small_into_big(s: &mut DieHardState) {
    let big = s.big;
    s.big = min(5, s.big + s.small);
    s.small = s.small - (s.big - big);
}

fn pour_big_into_small(s: &mut DieHardState) {
    let small = s.small;
    s.small = min(3, s.small + s.big);
    s.big = s.big - (s.small - small);
}


fn physics_of_jugs(s: &DieHardState) {
    assert!(0 <= s.small && s.small <= 3);
    assert!(0 <= s.big   && s.big   <= 5);
}

fn die_hard_problem_not_solved(s: &DieHardState) {
    note(&format!("small / big = {0} / {1}", s.small, s.big));
    assert!(s.big != 4);
}

#[hegel::test(test_cases = 1000)]
fn test_die_hard() {
    let rules: Vec<NamedRule<DieHardState>> = vec![
        ("fill_small", fill_small),
        ("fill_big", fill_big),
        ("empty_small", empty_small),
        ("empty_big", empty_big),
        ("pour_small_into_big", pour_small_into_big),
        ("pour_big_into_small", pour_big_into_small),
    ];
    let invariants: Vec<NamedInvariant<DieHardState>> = vec![
        ("physics", physics_of_jugs),
        ("not solved", die_hard_problem_not_solved),
    ];
    let machine = StateMachine {
        initializers: Vec::new(),
        rules: rules,
        invariants: invariants,
    };
    let initial_state = DieHardState {
        small: 0,
        big: 0,
    };
    hegel::stateful::run(machine, initial_state);
}

#[hegel::test]
fn test_machine_without_rules() {
    let machine = StateMachine {
        initializers: Vec::new(),
        rules: Vec::new(),
        invariants: Vec::new(),
    };
    let initial_state = 0;
    stateful::run(machine, initial_state);
}
