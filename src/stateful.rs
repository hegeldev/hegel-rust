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
use crate::control::{AssumeFailed, StopTest, raise_control};
use crate::generators::{Generator, integers};
use crate::runner::Mode;
use crate::test_case::raise_for_rc;
use parking_lot::Mutex;
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
///
/// Create one with [`pool()`] and populate it with [`add`](Pool::add). To draw
/// from the pool, use the generators it hands out rather than reading from it
/// directly:
///
/// - [`references`](Pool::references) returns a generator over `&T` — drawing
///   from it yields a reference to a value in the pool without removing it.
/// - [`values`](Pool::values) returns a generator over `T` — drawing from it
///   removes a value from the pool and yields it by value.
///
/// Both generators are used through [`tc.draw`](TestCase::draw), so the chosen
/// value is recorded in the failing-test replay and the choice shrinks like any
/// other draw.
pub struct Pool<T> {
    pool_id: i64,
    tc: TestCase,
    values: HashMap<i64, T>,
}

/// Ask the engine for a variable id from `pool_id`, consuming it if `consume`.
fn pool_generate(tc: &TestCase, pool_id: i64, consume: bool) -> i64 {
    match tc.with_ctc(|ctc| ctc.pool_generate(pool_id, consume)) {
        Ok(id) => id,
        Err(_) => raise_control(StopTest),
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
        let variable_id: i64 = match self.tc.with_ctc(|ctc| ctc.pool_add(self.pool_id)) {
            Ok(id) => id,
            Err(_) => raise_control(StopTest), // nocov
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
    pub fn references(&self) -> References<'_, T> {
        References {
            pool_id: self.pool_id,
            values: &self.values,
        }
    }

    /// A generator that consumes values from the pool.
    ///
    /// Drawing from it removes a value from the pool and yields it by value.
    /// Once consumed, that value is never drawn again. Drawing rejects the
    /// current test case (as if by `assume(false)`) when the pool is empty.
    pub fn values(&mut self) -> Values<'_, T> {
        Values {
            pool_id: self.pool_id,
            values: Mutex::new(&mut self.values),
        }
    }
}

/// A generator over references to the values in a [`Pool`].
///
/// Returned by [`Pool::references`]. Borrows the pool, so the references it
/// produces stay valid for as long as the generator is alive.
pub struct References<'a, T> {
    pool_id: i64,
    values: &'a HashMap<i64, T>,
}

impl<'a, T: Sync> Generator<&'a T> for References<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> &'a T {
        tc.assume(!self.values.is_empty());
        let variable_id = pool_generate(tc, self.pool_id, false);
        self.values.get(&variable_id).unwrap()
    }
}

/// A generator that consumes values from a [`Pool`], removing each value it
/// yields.
///
/// Returned by [`Pool::values`]. Borrows the pool mutably; the inner [`Mutex`]
/// is what lets it remove a value during a draw (which only has shared access
/// to the generator) while keeping the generator `Send + Sync`.
pub struct Values<'a, T> {
    pool_id: i64,
    values: Mutex<&'a mut HashMap<i64, T>>,
}

impl<T: Send> Generator<T> for Values<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let mut values = self.values.lock();
        tc.assume(!values.is_empty());
        let variable_id = pool_generate(tc, self.pool_id, true);
        values.remove(&variable_id).unwrap()
    }
}

/// Create a new value pool for stateful tests.
pub fn pool<T>(tc: &TestCase) -> Pool<T> {
    let pool_id = match tc.with_ctc(|ctc| ctc.new_pool()) {
        Ok(id) => id,
        Err(_) => raise_control(StopTest), // nocov
    };
    Pool {
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
    let rule_names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    let invariants = m.invariants();
    let invariant_names: Vec<&str> = invariants.iter().map(|r| r.name.as_str()).collect();
    let machine_id = match tc.with_ctc(|ctc| ctc.new_state_machine(&rule_names, &invariant_names)) {
        Ok(id) => id,
        Err(rc) => raise_for_rc(rc),
    };

    tc.note("Initial invariant check.");
    check_invariants(&mut m, &tc);

    let is_single = tc.mode() == Mode::SingleTestCase;

    let step_cap = if is_single {
        i64::MAX
    } else {
        let max_steps = 50;
        let unbounded_step_cap = tc.draw_silent(integers::<i64>().min_value(1));
        min(unbounded_step_cap, max_steps)
    };

    let mut steps_run_successfully = 0;
    let mut steps_attempted = 0;
    let mut step = 0;

    while steps_run_successfully < step_cap
        && (is_single
            || steps_attempted < 10 * step_cap
            || (steps_run_successfully == 0 && steps_attempted < 1000))
    {
        step += 1;
        let rule_index = match tc.with_ctc(|ctc| ctc.state_machine_next_rule(machine_id)) {
            Ok(i) => i as usize,
            Err(rc) => raise_for_rc(rc),
        };
        let rule = &rules[rule_index];
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
            // Backend ran out of data — this test case is done.
            Err(e) if e.downcast_ref::<StopTest>().is_some() => break,
            // Rule was skipped by assume(false); try a different rule.
            Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => {
                tc.note("Rule stopped early due to violated assumption.");
            }
            // Genuine rule failure: propagate it.
            Err(e) => resume_unwind(e),
        };
    }
}
