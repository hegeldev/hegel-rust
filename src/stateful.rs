//! Stateful (model-based) testing support.
//!
//! State machines are defined using the [`state_machine`](crate::state_machine) attribute macro.
//! Methods annotated with `#[rule]` become rules (actions applied to the state machine) and
//! methods annotated with `#[invariant]` become invariants. Invariants are checked on the
//! initial state, once after the final step, and occasionally between steps (after each
//! successful rule application with probability `1 / stateful_step_count`, so roughly once
//! per run) — a violation is always detected by the end of the run, without paying for a
//! full invariant check after every step. When a failure shrinks, the schedule shrinks
//! toward checking after every step, so the minimal counterexample fails at the earliest
//! violating step. Both kinds of method take a [`TestCase`] parameter and borrow the state
//! machine: rules
//! typically have signature `fn(&mut self, tc: TestCase)` and invariants
//! `fn(&self, tc: TestCase)`, but either kind of method may use `&self` or `&mut self`.
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
//!
//!     #[invariant]
//!     fn len_agrees_with_is_empty(&self, _: TestCase) {
//!         assert_eq!(self.stack.is_empty(), self.stack.len() == 0);
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
use crate::control::{AssumeFailed, hegel_internal_assert};
use crate::generators::Generator;
use crate::test_case::{labels, raise_for_rc};
use std::cell::RefCell;
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
///
/// Create one with [`pool()`] and populate it with [`add`](Pool::add). To draw
/// from the pool, use the generators it hands out rather than reading from it
/// directly:
///
/// - [`values_reusable`](Pool::values_reusable) returns a generator over `&T` —
///   drawing from it yields a reference to a value in the pool without removing
///   it.
/// - [`values_consumed`](Pool::values_consumed) returns a generator over `T` —
///   drawing from it removes a value from the pool and yields it by value.
///
/// Both generators are used through [`tc.draw`](TestCase::draw), so the chosen
/// value is recorded in the failing-test replay and the choice shrinks like any
/// other draw.
pub struct Pool<T> {
    pool: crate::ffi::PoolHandle,
    tc: TestCase,
    values: HashMap<i64, T>,
}

/// Ask the engine for a variable id from `pool`, consuming it if `consume`.
fn pool_generate(tc: &TestCase, pool: &crate::ffi::PoolHandle, consume: bool) -> i64 {
    match tc.with_ctc(|ctc| ctc.pool_generate(pool, consume)) {
        Ok(id) => id,
        Err(rc) => raise_for_rc(rc),
    }
}

impl<T> Pool<T> {
    /// Returns true if no values are in the pool.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Number of values currently in the pool.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Add a value to the pool.
    pub fn add(&mut self, v: T) {
        let variable_id: i64 = match self.tc.with_ctc(|ctc| ctc.pool_add(&self.pool)) {
            Ok(id) => id,
            Err(rc) => raise_for_rc(rc), // nocov
        };
        if self.values.contains_key(&variable_id) {
            panic!("unexpected variable id in map"); // nocov
        }
        self.values.insert(variable_id, v);
    }

    /// A generator over references to values in the pool.
    ///
    /// Drawing from it yields a `&T` borrowing a value in the pool, without
    /// removing it. Drawing rejects the current test case (as if by
    /// `assume(false)`) when the pool is empty.
    pub fn values_reusable(&self) -> ValuesReusable<'_, T> {
        ValuesReusable {
            pool: &self.pool,
            values: &self.values,
        }
    }

    /// A generator that consumes values from the pool.
    ///
    /// Drawing from it removes a value from the pool and yields it by value.
    /// Once consumed, that value is never drawn again. Drawing rejects the
    /// current test case (as if by `assume(false)`) when the pool is empty.
    pub fn values_consumed(&mut self) -> ValuesConsumed<'_, T> {
        ValuesConsumed {
            pool: &self.pool,
            values: RefCell::new(&mut self.values),
        }
    }
}

/// A generator over references to the values in a [`Pool`].
///
/// Returned by [`Pool::values_reusable`]. Borrows the pool, so the references it
/// produces stay valid for as long as the generator is alive.
pub struct ValuesReusable<'a, T> {
    pool: &'a crate::ffi::PoolHandle,
    values: &'a HashMap<i64, T>,
}

impl<'a, T> Generator<&'a T> for ValuesReusable<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> &'a T {
        tc.assume(!self.values.is_empty());
        let variable_id = pool_generate(tc, self.pool, false);
        self.values.get(&variable_id).unwrap()
    }
}

/// A generator that consumes values from a [`Pool`], removing each value it
/// yields.
///
/// Returned by [`Pool::values_consumed`]. Borrows the pool mutably; the inner
/// [`RefCell`] is what lets it remove a value during a draw, which only has
/// shared access to the generator.
pub struct ValuesConsumed<'a, T> {
    pool: &'a crate::ffi::PoolHandle,
    values: RefCell<&'a mut HashMap<i64, T>>,
}

impl<T> Generator<T> for ValuesConsumed<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        tc.assume(!self.values.borrow().is_empty());
        let variable_id = pool_generate(tc, self.pool, true);
        self.values.borrow_mut().remove(&variable_id).unwrap()
    }
}

/// Create a new value pool for stateful tests.
pub fn pool<T>(tc: &TestCase) -> Pool<T> {
    let pool = match tc.with_ctc(|ctc| ctc.new_pool()) {
        Ok(handle) => handle,
        Err(rc) => raise_for_rc(rc), // nocov
    };
    Pool {
        pool,
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
    /// Invariants checked on the initial state, at the end of the run, and
    /// probabilistically between steps (see the module docs).
    fn invariants(&self) -> Vec<Rule<Self>>;
}

fn check_invariants<M: StateMachine>(m: &mut M, invariants: &[Rule<M>], tc: &TestCase) {
    for invariant in invariants {
        let inv_tc = tc.child(2); // nocov
        (invariant.apply)(m, inv_tc); // nocov
    }
}

/// Execute a stateful test by repeatedly applying random rules and checking invariants.
pub fn run<M: StateMachine>(mut m: M, tc: TestCase) {
    let rules = m.rules();
    let rule_names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    let invariants = m.invariants();
    let invariant_names: Vec<&str> = invariants.iter().map(|r| r.name.as_str()).collect();
    let machine = match tc.with_ctc(|ctc| ctc.new_state_machine(&rule_names, &invariant_names)) {
        Ok(handle) => handle,
        Err(rc) => raise_for_rc(rc),
    };

    tc.note("Initial invariant check.");
    check_invariants(&mut m, &invariants, &tc);

    let skip_probability = 1.0 - 1.0 / (tc.stateful_step_count() as f64);
    let mut steps_attempted: i64 = 0;

    loop {
        tc.start_span(labels::STATEFUL_RULE);
        let rule_index = match tc.with_ctc(|ctc| ctc.state_machine_next_rule(&machine)) {
            Ok(Some(i)) => i,
            Ok(None) => break,
            Err(rc) => raise_for_rc(rc),
        };
        hegel_internal_assert!(
            (0..rules.len() as i64).contains(&rule_index),
            "state_machine_next_rule returned out-of-range rule index {rule_index}"
        );
        let rule = &rules[rule_index as usize];
        tc.note(&format!("Step {}: {}", steps_attempted + 1, rule.name));

        let rule_tc = tc.child(2);
        let thunk = || (rule.apply)(&mut m, rule_tc);
        let result = catch_unwind(AssertUnwindSafe(thunk));

        steps_attempted += 1;
        match result {
            Ok(()) => {
                tc.stop_span(false);
                // Invariants are sampled between steps with probability
                // 1 / stateful_step_count (they are guaranteed to run once
                // the run ends, below). The draw is inverted — false means
                // "check" — so that shrinking, which moves booleans toward
                // false, moves failing schedules toward checking invariants
                // after every step and so toward the earliest violating step.
                if !invariants.is_empty() && !tc.generate_boolean(skip_probability) {
                    check_invariants(&mut m, &invariants, &tc);
                }
            }
            Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => {
                if let Err(rc) = tc.with_ctc(|ctc| ctc.state_machine_rule_rejected(&machine)) {
                    raise_for_rc(rc);
                }
                tc.stop_span(true);
                tc.note("Rule stopped early due to violated assumption.");
            }
            // Everything else — including StopTest, so an out-of-data case is
            // reported as an overrun instead of returning normally with a
            // half-applied rule — unwinds through the caller.
            Err(e) => {
                tc.stop_span(false);
                resume_unwind(e)
            }
        };
    }

    check_invariants(&mut m, &invariants, &tc);
}
