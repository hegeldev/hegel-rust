#![allow(unused)]

use crate::generators::{integers, sampled_from};
use crate::control::{ASSUME_FAIL_STRING};
use crate::note;
use crate::hegel;
use crate::draw;
use std::cmp::{min, max};
use std::panic::{self, catch_unwind, resume_unwind, AssertUnwindSafe};

pub type Rule<T> = fn(&mut T);
pub type Invariant<T> = fn(&T);
pub type NamedRule<T> = (&'static str, Rule<T>);
pub type NamedInvariant<T> = (&'static str, Invariant<T>);

pub struct StateMachine<T> {
    pub initializers: Vec<NamedRule<T>>,
    pub rules: Vec<NamedRule<T>>,
    pub invariants: Vec<NamedInvariant<T>>,
}

fn check_invariants<T>(state: &T, invariants: &Vec<NamedInvariant<T>>) {
    for (name, invariant) in invariants {
        invariant(state);
    }
}

// TODO: factor out (shared with runner.rs)
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

pub fn run<T>(m: StateMachine<T>, mut s: T) {

    if m.rules.len() == 0 {
        panic!("Cannot run a machine with no rules.");
    }

    // TODO: The order should be randomized. Add a permutation generator!
    for (name, initializer) in &m.initializers {
        initializer(&mut s);
    }

    note("Initial invariant check");
    check_invariants(&s, &m.invariants);

    let rules = sampled_from(m.rules);

    // We generate an unbounded integer as the step cap that hypothesis actually sees. This means
    // we almost always run the maximum amount of steps, but allows us the possibility of shrinking
    // to a smaller number of steps.
    let max_steps = 50;
    let unbounded_step_cap = draw(&integers::<i64>().with_min(1));
    let step_cap = min(unbounded_step_cap, max_steps);

    let mut steps_run_successfully = 0;
    let mut steps_attempted = 0;

    // TODO: compare with the condition in the reference SDK
    while steps_run_successfully < step_cap && steps_attempted < 10 * step_cap {

        let (name, rule) = draw(&rules);

        // We only need this because AssertUnwindSafe expects a closure.
        let thunk = || rule(&mut s);
        let result = catch_unwind(AssertUnwindSafe(thunk));

        steps_attempted += 1;
        match result {
            Ok(()) => {
                steps_run_successfully += 1;
                check_invariants(&s, &m.invariants);
            },
            Err(e) => {
                if panic_message(&e) != ASSUME_FAIL_STRING {
                    resume_unwind(e);
                }
            },
        };

    }

}
