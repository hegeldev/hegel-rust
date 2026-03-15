use crate::generators::{integers, sampled_from};
use crate::test_case::ASSUME_FAIL_STRING;
use crate::TestCase;
use std::cmp::min;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

pub trait StateMachine {
    fn rules(&self) -> Vec<fn(&mut Self, &TestCase)>;
    fn invariants(&self) -> Vec<fn(&Self, &TestCase)>;
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

fn check_invariants(m: &impl StateMachine, tc: &TestCase) {
    let invariants = m.invariants();
    for invariant in invariants {
        invariant(m, tc);
    }
}

pub fn run(mut m: impl StateMachine, tc: TestCase) {
    let rules = m.rules();
    if rules.is_empty() {
        panic!("Cannot run a machine with no rules.");
    }

    let rules = &sampled_from(rules);

    tc.note("Initial invariant check.");
    check_invariants(&m, &tc);

    // We generate an unbounded integer as the step cap that hypothesis actually sees. This means
    // we almost always run the maximum amount of steps, but allows us the possibility of shrinking
    // to a smaller number of steps.
    let max_steps = 50;
    let unbounded_step_cap = tc.draw(integers::<i64>().min_value(1));
    let step_cap = min(unbounded_step_cap, max_steps);

    let mut steps_run_successfully = 0;
    let mut steps_attempted = 0;

    // TODO: compare with the condition in the reference SDK
    while steps_run_successfully < step_cap && steps_attempted < 10 * step_cap {
        let rule = tc.draw(rules);

        // We only need this because AssertUnwindSafe expects a closure.
        let thunk = || rule(&mut m, &tc);
        let result = catch_unwind(AssertUnwindSafe(thunk));

        steps_attempted += 1;
        match result {
            Ok(()) => {
                steps_run_successfully += 1;
                check_invariants(&m, &tc);
            }
            Err(e) => {
                if panic_message(&e) != ASSUME_FAIL_STRING {
                    resume_unwind(e);
                }
            }
        };
    }
}
