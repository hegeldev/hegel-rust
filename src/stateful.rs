//! Stateful (model-based) testing support.
//!
//! State machines are defined using the [`state_machine`](crate::state_machine) attribute macro.
//! Methods annotated with `#[rule]` become rules (actions applied to the state machine) and
//! methods annotated with `#[invariant]` become invariants (checked after each successful rule
//! application). Both take a [`TestCase`] parameter and borrow the state machine: rules
//! typically have signature `fn(&mut self, tc: TestCase)` and invariants
//! `fn(&self, tc: TestCase)`, but either kind of method may use `&self` or `&mut self`.
//!
//! To run a state machine, call [`run()`] inside a Hegel test.
//!
//! For *concurrent* stateful testing — rules applied to a shared model from
//! several worker threads at once — define the machine with the
//! [`concurrent_state_machine`](crate::concurrent_state_machine) attribute
//! macro instead and call [`run_concurrent()`] inside a test declared
//! `#[hegel::test(nondeterministic = true)]`. See [`run_concurrent()`] for
//! the execution model.
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
use crate::control::{
    AssumeFailed, InternalError, InvalidArgument, LoopDone, StopTest, hegel_internal_assert,
    raise_control, with_test_context,
};
use crate::generators::Generator;
use crate::run_lifecycle::{self, PanicInfo};
use crate::runner::Mode;
use crate::test_case::raise_for_rc;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Mutex, mpsc};

/// The concurrency group a `#[rule]` without a `group = "..."` argument is
/// assigned to. All unannotated rules share this one group, so a machine
/// with no group annotations is maximally concurrent — and in a machine
/// that mixes annotated and unannotated rules, the unannotated rules never
/// overlap with any named group's rules (see
/// [`ConcurrentStateMachine`]).
pub const ANONYMOUS_GROUP: &str = "<anonymous>";

thread_local! {
    /// The worker-thread index [`run_concurrent`] assigns to each of its
    /// worker threads, used to tag that worker's draw/note output lines.
    /// `None` outside a concurrent stateful worker.
    static WORKER_INDEX: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The calling thread's concurrent-worker index, if it is one of
/// [`run_concurrent`]'s worker threads. Read by the output machinery to tag
/// buffered lines so interleaved output stays readable.
pub(crate) fn current_worker_index() -> Option<usize> {
    WORKER_INDEX.with(|cell| cell.get())
}

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
    pool_id: i64,
    tc: TestCase,
    values: HashMap<i64, T>,
}

/// Ask the engine for a variable id from `pool_id`, consuming it if `consume`.
fn pool_generate(tc: &TestCase, pool_id: i64, consume: bool) -> i64 {
    match tc.with_ctc(|ctc| ctc.pool_generate(pool_id, consume)) {
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
        let variable_id: i64 = match self.tc.with_ctc(|ctc| ctc.pool_add(self.pool_id)) {
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
            pool_id: self.pool_id,
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
            pool_id: self.pool_id,
            values: RefCell::new(&mut self.values),
        }
    }
}

/// A generator over references to the values in a [`Pool`].
///
/// Returned by [`Pool::values_reusable`]. Borrows the pool, so the references it
/// produces stay valid for as long as the generator is alive.
pub struct ValuesReusable<'a, T> {
    pool_id: i64,
    values: &'a HashMap<i64, T>,
}

impl<'a, T> Generator<&'a T> for ValuesReusable<'a, T> {
    fn do_draw(&self, tc: &TestCase) -> &'a T {
        tc.assume(!self.values.is_empty());
        let variable_id = pool_generate(tc, self.pool_id, false);
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
    pool_id: i64,
    values: RefCell<&'a mut HashMap<i64, T>>,
}

impl<T> Generator<T> for ValuesConsumed<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        tc.assume(!self.values.borrow().is_empty());
        let variable_id = pool_generate(tc, self.pool_id, true);
        self.values.borrow_mut().remove(&variable_id).unwrap()
    }
}

/// Create a new value pool for stateful tests.
pub fn pool<T>(tc: &TestCase) -> Pool<T> {
    let pool_id = match tc.with_ctc(|ctc| ctc.new_pool()) {
        Ok(id) => id,
        Err(rc) => raise_for_rc(rc), // nocov
    };
    Pool {
        pool_id,
        tc: tc.clone(),
        values: HashMap::new(),
    }
}

/// A pool of previously generated values that [`run_concurrent`]'s worker
/// threads may share.
///
/// The concurrent counterpart of [`Pool`], designed for `&self` access from
/// rules: create one with [`concurrent_pool()`], store it in the model, and
/// call [`add`](ConcurrentPool::add) / draw from
/// [`values_reusable`](ConcurrentPool::values_reusable) /
/// [`values_consumed`](ConcurrentPool::values_consumed) from any worker.
/// `ConcurrentPool<T>` is `Sync` whenever `T: Send`, so a model holding one
/// satisfies [`run_concurrent`]'s `Sync` bound.
///
/// The engine performs each pool draw's empty check, selection, and
/// consumption atomically, so concurrent workers cannot double-consume a
/// value or race the emptiness check — exactly one worker receives each
/// consumed value. The accepted trade-off is that a shared pool couples the
/// workers' streams: which value a draw resolves to (and whether it rejects
/// as empty) depends on what other workers have added or consumed in the
/// meantime. In a nondeterministic run — the only place concurrency > 1
/// exists — nothing downstream depends on that independence anyway.
pub struct ConcurrentPool<T> {
    pool_id: i64,
    values: Mutex<HashMap<i64, T>>,
}

impl<T> ConcurrentPool<T> {
    /// Acquire the pool's map, recovering from poisoning: a panic can
    /// unwind while the guard is held (canonically a `Clone` impl panicking
    /// inside a reusable draw, which must clone under the lock), and
    /// letting that one panic turn every later pool operation on every
    /// worker into a `PoisonError` panic would bury the real failure under
    /// fake ones. Recovery is sound because no panic point sits between map
    /// mutations: a panicking clone reads the map without modifying it, and
    /// `add` inserts with an engine-issued id in a single operation, so the
    /// guarded map is consistent whenever the guard is droppable.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<i64, T>> {
        self.values.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns true if no values are in the pool.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Number of values currently in the pool.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Add a value to the pool. `tc` is the calling worker's test case.
    ///
    /// The pool's lock is held across the engine registration and the map
    /// insert, keeping the frontend map in lockstep with the engine's pool
    /// state: without this, a concurrent consumer could be handed a
    /// variable id whose value hasn't been inserted yet.
    pub fn add(&self, tc: &TestCase, v: T) {
        let mut values = self.lock();
        match tc.with_ctc(|ctc| ctc.pool_add(self.pool_id)) {
            Ok(variable_id) => {
                let previous = values.insert(variable_id, v);
                hegel_internal_assert!(previous.is_none(), "unexpected variable id in map");
            }
            Err(rc) => {
                drop(values);
                raise_for_rc(rc)
            }
        }
    }

    /// A generator over copies of the values in the pool.
    ///
    /// Drawing from it yields a *clone* of a value in the pool, without
    /// removing it — another worker may consume the referenced value at any
    /// moment, so references cannot safely escape the pool's lock; store
    /// `Arc<T>` in the pool if cloning is expensive. Drawing rejects the
    /// current test case (as if by `assume(false)`) when the pool is empty.
    pub fn values_reusable(&self) -> ConcurrentValuesReusable<'_, T> {
        ConcurrentValuesReusable { pool: self }
    }

    /// A generator that consumes values from the pool.
    ///
    /// Drawing from it removes a value from the pool and yields it by
    /// value; the engine consumes the id atomically, so exactly one worker
    /// receives each value. Drawing rejects the current test case (as if by
    /// `assume(false)`) when the pool is empty.
    pub fn values_consumed(&self) -> ConcurrentValuesConsumed<'_, T> {
        ConcurrentValuesConsumed { pool: self }
    }
}

/// A generator over cloned values in a [`ConcurrentPool`]. Returned by
/// [`ConcurrentPool::values_reusable`].
pub struct ConcurrentValuesReusable<'a, T> {
    pool: &'a ConcurrentPool<T>,
}

impl<T: Clone> Generator<T> for ConcurrentValuesReusable<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let values = self.pool.lock();
        match tc.with_ctc(|ctc| ctc.pool_generate(self.pool.pool_id, false)) {
            Ok(variable_id) => values.get(&variable_id).unwrap().clone(),
            Err(rc) => {
                drop(values);
                raise_for_rc(rc)
            }
        }
    }
}

/// A generator that consumes values from a [`ConcurrentPool`], removing
/// each value it yields. Returned by [`ConcurrentPool::values_consumed`].
pub struct ConcurrentValuesConsumed<'a, T> {
    pool: &'a ConcurrentPool<T>,
}

impl<T> Generator<T> for ConcurrentValuesConsumed<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        let mut values = self.pool.lock();
        match tc.with_ctc(|ctc| ctc.pool_generate(self.pool.pool_id, true)) {
            Ok(variable_id) => values.remove(&variable_id).unwrap(),
            Err(rc) => {
                drop(values);
                raise_for_rc(rc)
            }
        }
    }
}

/// Create a new value pool for concurrent stateful tests. See
/// [`ConcurrentPool`].
pub fn concurrent_pool<T>(tc: &TestCase) -> ConcurrentPool<T> {
    let pool_id = match tc.with_ctc(|ctc| ctc.new_pool()) {
        Ok(id) => id,
        Err(rc) => raise_for_rc(rc),
    };
    ConcurrentPool {
        pool_id,
        values: Mutex::new(HashMap::new()),
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

fn check_invariants<M: StateMachine>(m: &mut M, invariants: &[Rule<M>], tc: &TestCase) {
    for invariant in invariants {
        let inv_tc = tc.child(2); // nocov
        (invariant.apply)(m, inv_tc); // nocov
    }
}

/// Ask the engine whether the machine should run another round.
fn machine_next_group(tc: &TestCase, machine_id: i64) -> bool {
    match tc.with_ctc(|ctc| ctc.state_machine_next_group(machine_id)) {
        Ok(cont) => cont,
        Err(rc) => raise_for_rc(rc),
    }
}

/// Execute a stateful test by repeatedly applying random rules and checking invariants.
///
/// A sequential machine is the special case of the engine's concurrent
/// state-machine protocol with a single group and concurrency 1: the engine
/// hands out exactly one rule per round, so the join-point invariant check
/// after each round runs after each rule. One consequence of the join-point
/// timing: the invariants run after a rule that stopped on a violated
/// assumption too (rules are expected to reject before mutating the model,
/// and nothing restores model state on rejection anyway).
pub fn run<M: StateMachine>(mut m: M, tc: TestCase) {
    let rules = m.rules();
    let rule_names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    let rule_groups = vec![0i64; rules.len()];
    let invariants = m.invariants();
    let invariant_names: Vec<&str> = invariants.iter().map(|r| r.name.as_str()).collect();
    let machine_id = match tc.with_ctc(|ctc| {
        ctc.new_state_machine(
            &[ANONYMOUS_GROUP],
            &rule_names,
            &rule_groups,
            &invariant_names,
            1,
        )
    }) {
        Ok(id) => id,
        Err(rc) => raise_for_rc(rc),
    };

    tc.note("Initial invariant check.");
    check_invariants(&mut m, &invariants, &tc);

    let is_single = tc.mode() == Mode::SingleTestCase;

    let mut steps_attempted: i64 = 0;

    while is_single || steps_attempted < 1000 {
        if !machine_next_group(&tc, machine_id) {
            break;
        }

        loop {
            let rule_index = match tc.with_ctc(|ctc| ctc.state_machine_next_rule(machine_id, 0)) {
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
                Ok(()) => {}
                Err(e) if e.downcast_ref::<AssumeFailed>().is_some() => {
                    tc.note("Rule stopped early due to violated assumption.");
                }
                // Everything else — including StopTest, so an out-of-data
                // case is reported as an overrun instead of returning
                // normally with a half-applied rule — unwinds through the
                // caller.
                Err(e) => resume_unwind(e),
            };
        }

        check_invariants(&mut m, &invariants, &tc);
    }
}

/// A rule of a [`ConcurrentStateMachine`]: an action worker threads may
/// apply to the shared model during concurrent stateful testing.
///
/// Unlike a sequential [`Rule`], the apply function takes `&M`: the model is
/// shared by reference across the worker threads, so any mutable model
/// state needs interior mutability. `group` names the concurrency group the
/// rule belongs to; rules in the same group may run concurrently with each
/// other, rules in different groups never overlap.
pub struct ConcurrentRule<M: ?Sized> {
    pub name: String,
    pub group: String,
    pub apply: fn(&M, TestCase),
}

impl<M> ConcurrentRule<M> {
    /// Create a new rule with a name, a concurrency group, and an apply
    /// function. Pass [`ANONYMOUS_GROUP`] as the group for a rule without a
    /// group annotation.
    pub fn new(name: &str, group: &str, apply: fn(&M, TestCase)) -> Self {
        ConcurrentRule {
            name: name.to_string(),
            group: group.to_string(),
            apply,
        }
    }
}

/// An invariant of a [`ConcurrentStateMachine`], checked on the main thread
/// at every join point — between rounds of concurrent rule execution, while
/// all worker threads are parked.
pub struct ConcurrentInvariant<M: ?Sized> {
    pub name: String,
    pub apply: fn(&M, TestCase),
}

impl<M> ConcurrentInvariant<M> {
    /// Create a new invariant with a name and an apply function.
    pub fn new(name: &str, apply: fn(&M, TestCase)) -> Self {
        ConcurrentInvariant {
            name: name.to_string(),
            apply,
        }
    }
}

/// Trait for defining a concurrent stateful test.
///
/// Implement this to define the rules (actions), their concurrency-group
/// assignments, and the invariants of a model whose rules may run
/// *concurrently* against the system under test. Use
/// `#[hegel::concurrent_state_machine]` for a more ergonomic way to define
/// concurrent state machines, and [`run_concurrent`] to run one.
///
/// # Groups
///
/// At any moment exactly one group is *current*, and only rules belonging
/// to the current group are handed out — so rules in the same group may run
/// concurrently with each other, rules in different groups never overlap,
/// and the current group changes only at the join points between rounds.
/// Groups cannot express asymmetric overlap ("put may overlap get but not
/// delete"); that expressiveness limit is deliberate.
///
/// A rule without a group annotation is assigned to a single shared
/// anonymous group ([`ANONYMOUS_GROUP`]), so an unannotated machine is
/// maximally concurrent: any rule may overlap with any other, and naming
/// groups is how overlap gets *restricted*. **In a machine that mixes
/// annotated and unannotated rules, the unannotated rules form their own
/// group and therefore never overlap with any named group's rules** — do
/// not read "no group" as "unconstrained"; it is exactly backwards there.
pub trait ConcurrentStateMachine {
    /// The rules (actions) that worker threads may apply to this state
    /// machine, each with its concurrency-group assignment.
    fn rules(&self) -> Vec<ConcurrentRule<Self>>;
    /// Invariants checked at every join point, on the main thread, while
    /// the worker threads are parked.
    fn invariants(&self) -> Vec<ConcurrentInvariant<Self>>;
}

fn check_concurrent_invariants<M: ConcurrentStateMachine + ?Sized>(
    m: &M,
    invariants: &[ConcurrentInvariant<M>],
    tc: &TestCase,
) {
    for invariant in invariants {
        let inv_tc = tc.child(2);
        (invariant.apply)(m, inv_tc);
    }
}

/// A command from [`run_concurrent`]'s main thread to a worker.
enum WorkerCommand {
    /// A new round has begun: pull rules until the engine signals the join
    /// point, then report a [`WorkerEvent`].
    RunRound,
    /// The test case is over: exit the worker loop.
    Terminate,
}

/// What a worker reports back to the main thread at the end of its round.
enum WorkerEvent {
    /// The rule stream was exhausted normally.
    RoundDone,
    /// A control draw (`next_rule`) raised `AssumeFailed` — the engine
    /// concluded the family invalid mid-round (e.g. span nesting past its
    /// limit), so the whole case is invalid.
    Invalid,
    /// The family's choice budget is exhausted (`StopTest`): the whole case
    /// is an overrun.
    Overrun,
    /// A run-aborting control payload (`InternalError`, `InvalidArgument`)
    /// or a `LoopDone`, ferried verbatim for the main thread to re-raise.
    ControlPayload(Box<dyn std::any::Any + Send>),
    /// A real panic, with the worker-side capture to re-install on the main
    /// thread and its rendered location/message for noting dropped panics.
    Panicked {
        payload: Box<dyn std::any::Any + Send>,
        info: Option<PanicInfo>,
        location: String,
        message: String,
    },
    /// Synthesized by the main thread when a worker exited without
    /// reporting: never constructed by workers.
    Died,
}

/// How a caught unwind inside a worker classifies.
enum UnwindClass {
    Assume,
    Overrun,
    Control(Box<dyn std::any::Any + Send>),
    Panic(WorkerEvent),
}

/// Map an unwind caught outside any rule body — one raised by the round's
/// control draws — to the worker's terminal event: any `AssumeFailed` there
/// means the engine concluded the family invalid, so there is no rule to
/// skip and the whole case is invalid.
fn terminal_event(e: Box<dyn std::any::Any + Send>) -> WorkerEvent {
    match classify_worker_unwind(e) {
        UnwindClass::Assume => WorkerEvent::Invalid,
        UnwindClass::Overrun => WorkerEvent::Overrun,
        UnwindClass::Control(e) => WorkerEvent::ControlPayload(e),
        UnwindClass::Panic(event) => event,
    }
}

fn classify_worker_unwind(e: Box<dyn std::any::Any + Send>) -> UnwindClass {
    if e.downcast_ref::<AssumeFailed>().is_some() {
        return UnwindClass::Assume;
    }
    if e.downcast_ref::<StopTest>().is_some() {
        return UnwindClass::Overrun;
    }
    if e.downcast_ref::<InvalidArgument>().is_some()
        || e.downcast_ref::<InternalError>().is_some()
        || e.downcast_ref::<LoopDone>().is_some()
    {
        return UnwindClass::Control(e);
    }
    let info = run_lifecycle::take_panic_info();
    let location = info
        .as_ref()
        .map(|(_, _, location, _)| location.clone())
        .unwrap_or_else(|| "<unknown>".to_string());
    let message = run_lifecycle::panic_message(&e);
    UnwindClass::Panic(WorkerEvent::Panicked {
        payload: e,
        info,
        location,
        message,
    })
}

/// One worker's round: pull rules for `worker` until the engine signals the
/// join point or something terminal happens. Every unwind source — the
/// `next_rule` control draws included — runs under `catch_unwind`, so no
/// unwind ever escapes the worker thread: a worker that died without
/// reporting would leave the main thread parked forever waiting for its
/// event.
fn run_worker_round<M: ConcurrentStateMachine + ?Sized>(
    worker: usize,
    tc: &TestCase,
    m: &M,
    rules: &[ConcurrentRule<M>],
    machine_id: i64,
) -> WorkerEvent {
    loop {
        let next = catch_unwind(AssertUnwindSafe(|| {
            let next =
                match tc.with_ctc(|ctc| ctc.state_machine_next_rule(machine_id, worker as i64)) {
                    Ok(next) => next,
                    Err(rc) => raise_for_rc(rc),
                };
            if let Some(rule_index) = next {
                hegel_internal_assert!(
                    (0..rules.len() as i64).contains(&rule_index),
                    "state_machine_next_rule returned out-of-range rule index {rule_index}"
                );
            }
            next
        }));
        let rule_index = match next {
            Ok(Some(rule_index)) => rule_index,
            Ok(None) => return WorkerEvent::RoundDone,
            Err(e) => return terminal_event(e),
        };

        let rule = &rules[rule_index as usize];
        tc.note(&format!("Rule: {}", rule.name));
        let rule_tc = tc.child(2);
        let result = catch_unwind(AssertUnwindSafe(|| (rule.apply)(m, rule_tc)));
        match result {
            Ok(()) => {}
            Err(e) => match classify_worker_unwind(e) {
                UnwindClass::Assume => {
                    tc.note("Rule stopped early due to violated assumption.");
                }
                UnwindClass::Overrun => return WorkerEvent::Overrun,
                UnwindClass::Control(e) => return WorkerEvent::ControlPayload(e),
                UnwindClass::Panic(event) => return event,
            },
        }
    }
}

/// A worker thread's whole-test-case loop: enter the test context (the
/// panic hook captures nothing on a thread outside it, and internal errors
/// must raise catchably), mirror the main thread's backtrace-capture
/// setting, then run a round per [`WorkerCommand::RunRound`] until told to
/// terminate.
#[allow(clippy::too_many_arguments)]
fn worker_loop<M: ConcurrentStateMachine + ?Sized>(
    worker: usize,
    tc: TestCase,
    m: &M,
    rules: &[ConcurrentRule<M>],
    machine_id: i64,
    capture_backtraces: bool,
    commands: mpsc::Receiver<WorkerCommand>,
    events: mpsc::Sender<WorkerEvent>,
) {
    WORKER_INDEX.with(|cell| cell.set(Some(worker)));
    run_lifecycle::set_backtrace_capture(capture_backtraces);
    with_test_context(|| {
        loop {
            match commands.recv() {
                Ok(WorkerCommand::RunRound) => {
                    let event = run_worker_round(worker, &tc, m, rules, machine_id);
                    if events.send(event).is_err() {
                        break;
                    }
                }
                Ok(WorkerCommand::Terminate) | Err(_) => break,
            }
        }
    });
}

/// Signals termination to every worker when dropped, so *any* exit from the
/// scope body — the normal end of the test case or an unwind from a join
/// point (a panicking invariant, an invariant's `AssumeFailed`, a draw that
/// exhausts the budget) — wakes the parked workers and lets the scope's
/// implicit join complete instead of hanging. Sending is idempotent:
/// workers exit on the first `Terminate` they see, and sends to an
/// already-exited worker fail harmlessly.
struct TerminationGuard<'a> {
    round_txs: &'a [mpsc::Sender<WorkerCommand>],
}

impl Drop for TerminationGuard<'_> {
    fn drop(&mut self) {
        for tx in self.round_txs {
            let _ = tx.send(WorkerCommand::Terminate);
        }
    }
}

/// Execute a concurrent stateful test: repeatedly run rounds of rules from
/// the current concurrency group on `concurrency` worker threads, checking
/// invariants at the join points between rounds.
///
/// The engine draws the actual concurrency level for each test case, in
/// `[1, max_concurrency]` and weighted toward `max_concurrency`
/// (concurrency bugs need concurrency). The model is shared by reference
/// across the worker threads, so rules and invariants take `&self` and any
/// mutable model state needs interior mutability (locks, atomics, a
/// [`ConcurrentPool`], ...).
///
/// # The run must be declared nondeterministic
///
/// Concurrency bugs are nondeterministic — thread scheduling is outside
/// Hegel's control — so a test using `run_concurrent` must declare its run
/// nondeterministic *statically*, via `#[hegel::test(nondeterministic =
/// true)]` (or [`Settings::nondeterministic`](crate::Settings::nondeterministic)
/// for non-macro users). Failures are then reported faithfully from the
/// discovering execution, with no replay, shrinking, flakiness complaints,
/// database persistence, or reproduce blob — and at most one failure per
/// run. This function panics up front when the run has not been declared
/// nondeterministic.
///
/// # Abandoned rules and lock poisoning
///
/// A rule abandoned mid-execution by an unwind — most routinely a rejected
/// assumption: an empty-pool draw *is* an assume violation by design — can
/// poison any `std::sync::Mutex` it was holding, and every other worker's
/// `lock().unwrap()` then panics with a `PoisonError` that no real schedule
/// of your rules could produce. Two rules of thumb prevent those fake
/// failures:
///
/// - **Draw before locking**: complete all of a rule's draws (including
///   pool draws, which reject when the pool is empty) before taking any
///   lock, so a rejection can never unwind through a held guard.
/// - **Make model locks poison-tolerant**: recover with
///   `lock().unwrap_or_else(|e| e.into_inner())`, or use a non-poisoning
///   lock (e.g. `parking_lot`). Then even a mid-rule *panic* can't turn
///   later lock acquisitions into fake `PoisonError` failures; the
///   half-mutated state a recovered lock may expose is precisely what the
///   invariants are there to catch, and the original panic is still the one
///   reported.
///
/// [`ConcurrentPool`] follows both rules itself.
///
/// # Example
///
/// ```no_run
/// use std::sync::Mutex;
/// use hegel::TestCase;
/// use hegel::generators as gs;
///
/// struct CounterTest {
///     counter: Mutex<i64>,
/// }
///
/// #[hegel::concurrent_state_machine]
/// impl CounterTest {
///     #[rule(group = "write")]
///     fn increment(&self, _: TestCase) {
///         *self.counter.lock().unwrap_or_else(|e| e.into_inner()) += 1;
///     }
///
///     #[rule(group = "read")]
///     fn read(&self, _: TestCase) {
///         let _ = *self.counter.lock().unwrap_or_else(|e| e.into_inner());
///     }
///
///     #[invariant]
///     fn non_negative(&self, _: TestCase) {
///         assert!(*self.counter.lock().unwrap_or_else(|e| e.into_inner()) >= 0);
///     }
/// }
///
/// #[hegel::test(nondeterministic = true)]
/// fn test_counter(tc: TestCase) {
///     let m = CounterTest { counter: Mutex::new(0) };
///     hegel::stateful::run_concurrent(m, tc, 3);
/// }
/// ```
pub fn run_concurrent<M: ConcurrentStateMachine + Sync>(m: M, tc: TestCase, max_concurrency: i64) {
    if !tc.nondeterministic() {
        raise_control(InvalidArgument(
            "stateful::run_concurrent requires the run to be declared nondeterministic: \
             concurrent stateful tests cannot be replayed or shrunk deterministically. \
             Declare it with #[hegel::test(nondeterministic = true)], or with \
             Settings::new().nondeterministic(true) when configuring the run by hand."
                .to_string(),
        ));
    }

    let rules = m.rules();
    let invariants = m.invariants();
    let rule_names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    let invariant_names: Vec<&str> = invariants.iter().map(|r| r.name.as_str()).collect();
    let mut group_names: Vec<&str> = Vec::new();
    let mut rule_groups: Vec<i64> = Vec::with_capacity(rules.len());
    for rule in &rules {
        let index = group_names
            .iter()
            .position(|name| *name == rule.group)
            .unwrap_or_else(|| {
                group_names.push(rule.group.as_str());
                group_names.len() - 1
            });
        rule_groups.push(index as i64);
    }

    let concurrency = match tc.with_ctc(|ctc| ctc.generate_concurrency(max_concurrency)) {
        Ok(level) => level,
        Err(rc) => raise_for_rc(rc),
    };
    tc.note(&format!("Concurrency level: {concurrency}"));
    let machine_id = match tc.with_ctc(|ctc| {
        ctc.new_state_machine(
            &group_names,
            &rule_names,
            &rule_groups,
            &invariant_names,
            concurrency,
        )
    }) {
        Ok(id) => id,
        Err(rc) => raise_for_rc(rc),
    };

    tc.note("Initial invariant check.");
    check_concurrent_invariants(&m, &invariants, &tc);

    let capture_backtraces = run_lifecycle::backtrace_capture_enabled();
    let concurrency = concurrency as usize;
    let m = &m;
    let rules = &rules;

    std::thread::scope(|scope| {
        let mut round_txs: Vec<mpsc::Sender<WorkerCommand>> = Vec::with_capacity(concurrency);
        let mut event_rxs: Vec<mpsc::Receiver<WorkerEvent>> = Vec::with_capacity(concurrency);
        for worker in 0..concurrency {
            let (round_tx, round_rx) = mpsc::channel();
            let (event_tx, event_rx) = mpsc::channel();
            round_txs.push(round_tx);
            event_rxs.push(event_rx);
            let worker_tc = tc.clone();
            scope.spawn(move || {
                worker_loop(
                    worker,
                    worker_tc,
                    m,
                    rules,
                    machine_id,
                    capture_backtraces,
                    round_rx,
                    event_tx,
                );
            });
        }
        let _guard = TerminationGuard {
            round_txs: &round_txs,
        };

        loop {
            if !machine_next_group(&tc, machine_id) {
                break;
            }

            for tx in &round_txs {
                let _ = tx.send(WorkerCommand::RunRound);
            }
            let events: Vec<WorkerEvent> = event_rxs
                .iter()
                .map(|rx| rx.recv().unwrap_or(WorkerEvent::Died))
                .collect();

            resolve_round(events, &tc);

            check_concurrent_invariants(m, &invariants, &tc);
        }
    });
}

/// Classify one round's worker events and, for a terminal round, re-raise
/// on the main thread. Precedence: **control payloads win over overrun, and
/// overrun wins over panic** — a control payload signals a framework or
/// usage bug that must not be masked, and a panic that co-occurs with an
/// engine-side family conclusion (overrun, invalid) is not trustworthy: the
/// concluding draw abandoned a rule mid-execution, and that abandonment's
/// side effects (canonically a poisoned model lock) can induce panics in
/// other workers that no real schedule of the user's rules could produce.
/// Misclassification costs are asymmetric, too: a dropped genuine panic
/// merely discards this case and resurfaces in a later one, while a
/// reported fake panic halts the run with a false bug. Dropped panics are
/// noted into the case's output buffer, so they stay visible whenever that
/// buffer is shown. Among several panics, the lowest worker index wins.
fn resolve_round(events: Vec<WorkerEvent>, tc: &TestCase) {
    struct WorkerPanic {
        worker: usize,
        payload: Box<dyn std::any::Any + Send>,
        info: Option<PanicInfo>,
        location: String,
        message: String,
    }

    let mut control: Option<Box<dyn std::any::Any + Send>> = None;
    let mut saw_overrun = false;
    let mut saw_invalid = false;
    let mut panics: Vec<WorkerPanic> = Vec::new();
    for (worker, event) in events.into_iter().enumerate() {
        match event {
            WorkerEvent::RoundDone => {}
            WorkerEvent::Invalid => saw_invalid = true,
            WorkerEvent::Overrun => saw_overrun = true,
            WorkerEvent::ControlPayload(payload) => {
                if control.is_none() {
                    control = Some(payload);
                }
            }
            WorkerEvent::Panicked {
                payload,
                info,
                location,
                message,
            } => panics.push(WorkerPanic {
                worker,
                payload,
                info,
                location,
                message,
            }),
            WorkerEvent::Died => {
                if control.is_none() {
                    control = Some(Box::new(InternalError(format!(
                        "Internal error in hegel: concurrent stateful worker {worker} exited \
                         without reporting an outcome. This is a bug in hegel itself; please \
                         report it at https://github.com/hegeldev/hegel-rust/issues"
                    ))));
                }
            }
        }
    }

    let note_dropped = |dropped: &[WorkerPanic]| {
        for p in dropped {
            tc.note(&format!(
                "Dropped concurrent panic from worker {} at {}: {}",
                p.worker, p.location, p.message
            ));
        }
    };

    if let Some(payload) = control {
        resume_unwind(payload);
    }
    if saw_overrun || saw_invalid {
        note_dropped(&panics);
        if saw_overrun {
            raise_control(StopTest);
        }
        raise_control(AssumeFailed);
    }
    if !panics.is_empty() {
        note_dropped(&panics[1..]);
        let winner = panics.remove(0);
        if let Some(info) = winner.info {
            run_lifecycle::install_panic_info(info);
        }
        resume_unwind(winner.payload);
    }
}

#[cfg(test)]
#[path = "../tests/embedded/stateful_tests.rs"]
mod tests;
