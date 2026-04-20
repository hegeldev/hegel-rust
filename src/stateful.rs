//! Stateful (model-based) testing support.
//!
//! State machines are defined using the [`state_machine`](crate::state_machine) attribute macro.
//! Methods annotated with `#[rule]` become rules (actions applied to the state machine) and
//! methods annotated with `#[invariant]` become invariants (checked after each successful rule
//! application). Rules must have signature `fn(&mut self, tc: TestCase)` and invariants must have
//! signature `fn(&self, tc: TestCase)`.
//!
//! To run a state machine, call [`run()`] inside a Hegel test.
//!
//! Example:
//! ```rust
//! use hegel::TestCase;
//! use hegel::generators as gs;
//!
//! struct IntegerStack {
//!     stack: Vec<i32>,
//! }
//!
//! #[hegel::state_machine]
//! impl IntegerStack {
//!     #[rule]
//!     fn push(&mut self, tc: TestCase) {
//!         let integers = gs::integers::<i32>;
//!         let element = tc.draw(integers());
//!         self.stack.push(element);
//!     }
//!
//!     #[rule]
//!     fn pop(&mut self, _: TestCase) {
//!         self.stack.pop();
//!     }
//!
//!     #[rule]
//!     fn pop_push(&mut self, tc: TestCase) {
//!         let integers = gs::integers::<i32>;
//!         let element = tc.draw(integers());
//!         let initial = self.stack.clone();
//!         self.stack.push(element);
//!         let popped = self.stack.pop().unwrap();
//!         assert_eq!(popped, element);
//!         assert_eq!(self.stack, initial);
//!     }
//!
//!     #[rule]
//!     fn push_pop(&mut self, tc: TestCase) {
//!         let initial = self.stack.clone();
//!         let element = self.stack.pop();
//!         tc.assume(element.is_some());
//!         let element = element.unwrap();
//!         self.stack.push(element);
//!         assert_eq!(self.stack, initial);
//!     }
//! }
//!
//! #[hegel::test]
//! fn test_integer_stack(tc: TestCase) {
//!     let stack = IntegerStack { stack: Vec::new() };
//!     hegel::stateful::run(stack, tc);
//! }
//! ```

use crate::TestCase;
use crate::generators::integers;
use crate::test_case::{ASSUME_FAIL_STRING, STOP_TEST_STRING};
use std::cmp::min;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

/// A rule that can be applied to the state machine during testing.
pub struct Rule<M: ?Sized> {
    pub name: String,
    pub apply: fn(&mut M, TestCase),
}

impl<M> Rule<M> {
    /// Create a new rule with a name and an apply function.
    pub fn new(name: &str, apply: fn(&mut M, TestCase)) -> Self {
        Rule {
            name: name.to_string(),
            apply,
        }
    }
}

/// A pool of previously generated values.
pub struct Variables<T> {
    pool_id: i128,
    tc: TestCase,
    values: HashMap<i128, T>,
}

impl<T> Variables<T> {
    fn pool_generate(&self, consume: bool) -> i128 {
        match self
            .tc
            .with_data_source(|ds| ds.pool_generate(self.pool_id, consume))
        {
            Ok(id) => id,
            Err(_) => {
                panic!("{}", STOP_TEST_STRING);
            }
        }
    }

    /// Returns true if no variables are in the pool.
    pub fn empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Add a value to the pool.
    pub fn add(&mut self, v: T) {
        let variable_id: i128 = match self.tc.with_data_source(|ds| ds.pool_add(self.pool_id)) {
            Ok(id) => id,
            Err(_) => {
                panic!("{}", STOP_TEST_STRING); // nocov
            }
        };
        if self.values.contains_key(&variable_id) {
            panic!("unexpected variable id in map"); // nocov
        }
        self.values.insert(variable_id, v);
    }

    /// Draw a reference to a value from the pool (without removing it).
    ///
    /// Calls `assume(false)` if the pool is empty.
    pub fn draw(&self) -> &T {
        self.tc.assume(!self.empty());
        let variable_id = self.pool_generate(false);
        self.values.get(&variable_id).unwrap()
    }

    /// Remove and return a value from the pool.
    ///
    /// Calls `assume(false)` if the pool is empty.
    pub fn consume(&mut self) -> T {
        self.tc.assume(!self.empty());
        let variable_id = self.pool_generate(true);
        self.values.remove(&variable_id).unwrap()
    }
}

/// Create a new variable pool for stateful tests.
pub fn variables<T>(tc: &TestCase) -> Variables<T> {
    let pool_id = match tc.with_data_source(|ds| ds.new_pool()) {
        Ok(id) => id,
        Err(_) => {
            panic!("{}", STOP_TEST_STRING); // nocov
        }
    };
    Variables {
        pool_id,
        tc: tc.clone(),
        values: HashMap::new(),
    }
}

/// Trait for defining a stateful test.
///
/// Implement this to define the rules (actions) and invariants (assertions)
/// of your state machine. Use `#[hegel::state_machine]` for a more
/// ergonomic way to define state machines.
pub trait StateMachine {
    /// The rules (actions) that can be applied to this state machine.
    fn rules(&self) -> Vec<Rule<Self>>;
    /// Invariants checked after each successful rule application.
    fn invariants(&self) -> Vec<Rule<Self>>;
}

// TODO: factor out (shared with runner.rs)
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string() // nocov
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string() // nocov
    }
}

fn check_invariants(m: &mut impl StateMachine, tc: &TestCase) {
    let invariants = m.invariants();
    for invariant in invariants {
        let inv_tc = tc.child(2); // nocov
        (invariant.apply)(m, inv_tc); // nocov
    }
}

/// Execute a stateful test by repeatedly applying random rules and checking invariants.
pub fn run(mut m: impl StateMachine, tc: TestCase) {
    let rules = m.rules();
    if rules.is_empty() {
        panic!("Cannot run a machine with no rules."); // nocov
    }

    let rule_index = integers::<usize>().min_value(0).max_value(rules.len() - 1);

    tc.note("Initial invariant check.");
    check_invariants(&mut m, &tc);

    // We generate an unbounded integer as the step cap that hypothesis actually sees. This means
    // we almost always run the maximum amount of steps, but allows us the possibility of shrinking
    // to a smaller number of steps.
    let max_steps = 50;
    let unbounded_step_cap = tc.draw_silent(integers::<i64>().min_value(1));
    let step_cap = min(unbounded_step_cap, max_steps);

    let mut steps_run_successfully = 0;
    let mut steps_attempted = 0;
    let mut step = 0;

    while steps_run_successfully < step_cap
        && (steps_attempted < 10 * step_cap
            || (steps_run_successfully == 0 && steps_attempted < 1000))
    {
        step += 1;
        let rule = &rules[tc.draw_silent(&rule_index)];
        tc.note(&format!("Step {}: {}", step, rule.name));

        // We only need this because AssertUnwindSafe expects a closure.
        let rule_tc = tc.child(2);
        let thunk = || (rule.apply)(&mut m, rule_tc);
        let result = catch_unwind(AssertUnwindSafe(thunk));

        steps_attempted += 1;
        match result {
            Ok(()) => {
                steps_run_successfully += 1;
                check_invariants(&mut m, &tc);
            }
            Err(e) => {
                let msg = panic_message(&e);
                if msg == STOP_TEST_STRING {
                    // Backend ran out of data — this test case is done.
                    break;
                } else if msg != ASSUME_FAIL_STRING {
                    tc.note("Rule stopped early due to violated assumption.");
                    resume_unwind(e);
                }
            }
        };
    }
}
