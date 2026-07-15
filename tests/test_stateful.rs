mod common;

use common::project::TempRustProject;
use hegel::TestCase;
use hegel::generators as gs;
use hegel::stateful::{Pool, pool};

#[test]
fn test_state_machine_failure() {
    let code = r#"
use hegel::TestCase;

struct Linear {
    state: i32,
}

#[hegel::state_machine]
impl Linear {
    #[rule]
    fn zero(&mut self, tc: TestCase) {
        tc.assume(self.state == 0);
        self.state += 1;
    }

    #[rule]
    fn one(&mut self, tc: TestCase) {
        tc.assume(self.state == 1);
        self.state += 1;
    }

    #[rule]
    fn two(&mut self, tc: TestCase) {
        tc.assume(self.state == 2);
        self.state += 1;
    }

    #[rule]
    fn three(&mut self, tc: TestCase) {
        tc.assume(self.state == 3);
        self.state += 1;
    }

    #[invariant]
    fn upper_bound(&mut self, _tc: TestCase) {
        assert!(self.state < 4);
    }
}

#[hegel::test]
fn test_upper_bound(tc: TestCase) {
    let m = Linear { state: 0 };
    hegel::stateful::run(m, tc);
}

fn main() {}
"#;

    TempRustProject::new()
        .main_file(code)
        .expect_failure("assertion failed: self.state < 4")
        .cargo_test(&[]);
}

struct TestConsumeMachine {
    numbers: Pool<i32>,
    consumed: i32,
}

#[hegel::state_machine]
impl TestConsumeMachine {
    #[rule]
    fn draw(&mut self, tc: TestCase) {
        let x = tc.draw(self.numbers.values_reusable());
        assert!(*x != self.consumed);
    }
}

#[hegel::test]
fn test_consume(tc: TestCase) {
    let ints = gs::integers::<i32>;
    let elements = tc.draw(gs::vecs(ints()).unique(true));
    tc.assume(!elements.is_empty());
    let mut bundle = pool(&tc);
    for element in elements.clone() {
        bundle.add(element);
    }
    let consumed = tc.draw(bundle.values_consumed());
    let m = TestConsumeMachine {
        numbers: bundle,
        consumed,
    };
    hegel::stateful::run(m, tc);
}

struct TestLifetimeMachine<'a> {
    data: &'a [i32],
}

#[hegel::state_machine]
impl<'a> TestLifetimeMachine<'a> {
    #[rule]
    fn f(&mut self, _tc: TestCase) {
        assert!(!self.data.is_empty());
    }
}

#[hegel::test]
fn test_state_machine_with_lifetime(tc: TestCase) {
    let data = vec![1, 2, 3];
    let m = TestLifetimeMachine { data: &data };
    hegel::stateful::run(m, tc);
}

struct GenericMachine<T> {
    values: Vec<T>,
}

#[hegel::state_machine]
impl<T: std::fmt::Debug> GenericMachine<T> {
    #[rule]
    fn check(&mut self, _tc: TestCase) {
        let _ = self.values.len();
    }
}

#[hegel::test]
fn test_state_machine_with_type_parameter(tc: TestCase) {
    let m = GenericMachine {
        values: vec![1, 2, 3],
    };
    hegel::stateful::run(m, tc);
}

struct TestDrawDomainMachine {
    domain: Vec<i32>,
    pool: Pool<i32>,
}

#[hegel::state_machine]
impl TestDrawDomainMachine {
    #[rule]
    fn draw(&mut self, tc: TestCase) {
        let x = tc.draw(self.pool.values_reusable());
        assert!(self.domain.contains(x));
    }

    #[invariant]
    fn len_matches_domain(&mut self, _tc: TestCase) {
        assert!(!self.pool.is_empty());
        assert_eq!(self.pool.len(), self.domain.len());
    }
}

#[hegel::test]
fn test_draw_domain(tc: TestCase) {
    let ints = gs::integers::<i32>;
    let elements = tc.draw(gs::vecs(ints()));
    tc.assume(!elements.is_empty());
    let mut bundle = pool(&tc);
    for element in elements.clone() {
        bundle.add(element);
    }
    let m = TestDrawDomainMachine {
        domain: elements,
        pool: bundle,
    };
    hegel::stateful::run(m, tc);
}

mod stateful {
    use super::common::project::TempRustProject;
    use super::common::utils::expect_panic;
    use hegel::TestCase;
    use hegel::generators as gs;
    use hegel::stateful::{Pool, Rule, StateMachine, pool};
    use hegel::{Hegel, Settings, Verbosity};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, Mutex};

    /// Run `body` as a Hegel property test and return the lines emitted through
    /// the output sink. With `verbose` set, every test case emits its notes and
    /// draws; otherwise only the final replay of a failing case does. `body` is
    /// run inside `with_output_override` so those lines are captured instead of
    /// going to stderr.
    fn capture_output<F>(verbose: bool, body: F) -> String
    where
        F: FnMut(TestCase) + 'static,
    {
        let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let buf_writer = buf.clone();
        let sink: Arc<dyn Fn(&str) + Send + Sync> =
            Arc::new(move |s: &str| buf_writer.lock().unwrap().push(s.to_string()));

        let verbosity = if verbose {
            Verbosity::Verbose
        } else {
            Verbosity::Normal
        };

        let _ = catch_unwind(AssertUnwindSafe(|| {
            hegel::with_output_override(sink, || {
                Hegel::new(body)
                    .settings(
                        Settings::new()
                            .test_cases(200)
                            .database(None)
                            .derandomize(true)
                            .verbosity(verbosity),
                    )
                    .run();
            });
        }));

        buf.lock().unwrap().join("\n")
    }

    struct GenuineFailureMachine;

    #[hegel::state_machine]
    impl GenuineFailureMachine {
        #[rule]
        fn always_fails(&mut self, _tc: TestCase) {
            panic!("boom");
        }
    }

    #[test]
    fn test_genuine_failure_is_not_reported_as_violated_assumption() {
        let output = capture_output(false, |tc: TestCase| {
            hegel::stateful::run(GenuineFailureMachine, tc);
        });
        assert!(
            output.contains("Step 1: always_fails"),
            "expected the failing step to appear in the replay output:\n{output}"
        );
        assert!(
            !output.contains("violated assumption"),
            "a genuine rule failure must not be reported as a violated assumption:\n{output}"
        );
    }

    struct AssumeSkipMachine;

    #[hegel::state_machine]
    impl AssumeSkipMachine {
        #[rule]
        fn succeeds(&mut self, _tc: TestCase) {}

        #[rule]
        fn skips(&mut self, tc: TestCase) {
            tc.assume(false);
        }
    }

    #[test]
    fn test_assume_skip_is_reported_as_violated_assumption() {
        let output = capture_output(true, |tc: TestCase| {
            hegel::stateful::run(AssumeSkipMachine, tc);
        });
        assert!(
            output.contains("violated assumption"),
            "a rule skipped via assume(false) should be reported as a violated assumption:\n{output}"
        );
    }

    #[derive(Debug)]
    struct DepthCharge {
        depth: i64,
    }

    struct DepthMachine {
        charges: Pool<DepthCharge>,
    }

    #[hegel::state_machine]
    impl DepthMachine {
        #[rule]
        fn charge(&mut self, tc: TestCase) {
            let depth = tc.draw(self.charges.values_reusable()).depth;
            self.charges.add(DepthCharge { depth: depth + 1 });
        }

        #[rule]
        fn none_charge(&mut self, _tc: TestCase) {
            self.charges.add(DepthCharge { depth: 0 });
        }

        #[rule]
        fn is_not_too_deep(&mut self, tc: TestCase) {
            let check = tc.draw(self.charges.values_reusable());
            assert!(check.depth < 3, "depth {} is not less than 3", check.depth);
        }
    }

    struct InvariantMachine;

    #[hegel::state_machine]
    impl InvariantMachine {
        #[invariant]
        fn test_blah(&mut self, _tc: TestCase) {
            panic!("invariant always fails");
        }

        #[rule]
        fn do_stuff(&mut self, _tc: TestCase) {}
    }

    #[test]
    fn test_invariant() {
        expect_panic(
            || {
                Hegel::new(|tc: TestCase| {
                    hegel::stateful::run(InvariantMachine, tc);
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "invariant always fails",
        );
    }

    struct MultipleInvariantMachine {
        first_ran: bool,
    }

    #[hegel::state_machine]
    impl MultipleInvariantMachine {
        #[invariant]
        fn invariant_1(&mut self, _tc: TestCase) {
            self.first_ran = true;
        }

        #[invariant]
        fn invariant_2(&mut self, _tc: TestCase) {
            if self.first_ran {
                panic!("all invariants ran");
            }
        }

        #[rule]
        fn do_stuff(&mut self, _tc: TestCase) {}
    }

    #[test]
    fn test_multiple_invariants() {
        expect_panic(
            || {
                Hegel::new(|tc: TestCase| {
                    hegel::stateful::run(MultipleInvariantMachine { first_ran: false }, tc);
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "all invariants ran",
        );
    }

    struct InitialStateMachine {
        num: i64,
    }

    #[hegel::state_machine]
    impl InitialStateMachine {
        #[invariant]
        fn test_blah(&mut self, _tc: TestCase) {
            if self.num == 0 {
                panic!("num is 0 in invariant");
            }
        }

        #[rule]
        fn test_foo(&mut self, _tc: TestCase) {
            self.num += 1;
        }
    }

    #[test]
    fn test_invariant_checks_initial_state_if_no_initialize_rules() {
        expect_panic(
            || {
                Hegel::new(|tc: TestCase| {
                    hegel::stateful::run(InitialStateMachine { num: 0 }, tc);
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "num is 0 in invariant",
        );
    }

    struct CountStepsMachine {
        count: std::sync::Arc<std::sync::Mutex<i64>>,
    }

    impl StateMachine for CountStepsMachine {
        fn rules(&self) -> Vec<Rule<Self>> {
            vec![Rule::new(
                "do_something",
                |m: &mut CountStepsMachine, _tc: TestCase| {
                    *m.count.lock().unwrap() += 1;
                },
            )]
        }
        fn invariants(&self) -> Vec<Rule<Self>> {
            vec![]
        }
    }

    #[test]
    fn test_always_runs_at_least_one_step() {
        Hegel::new(|tc: TestCase| {
            let count = std::sync::Arc::new(std::sync::Mutex::new(0i64));
            let count_check = std::sync::Arc::clone(&count);
            let m = CountStepsMachine { count };
            hegel::stateful::run(m, tc);
            assert!(
                *count_check.lock().unwrap() > 0,
                "at least one step must run before teardown"
            );
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    struct RequiresInit {
        threshold: i64,
    }

    #[hegel::state_machine]
    impl RequiresInit {
        #[rule]
        fn action(&mut self, tc: TestCase) {
            let value = tc.draw(gs::integers::<i64>());
            if value > self.threshold {
                panic!("{} is too high", value);
            }
        }
    }

    #[test]
    fn test_can_use_factory_for_tests() {
        expect_panic(
            || {
                Hegel::new(|tc: TestCase| {
                    hegel::stateful::run(RequiresInit { threshold: 42 }, tc);
                })
                .settings(Settings::new().database(None).test_cases(100))
                .run();
            },
            "is too high",
        );
    }

    #[test]
    fn test_can_run_with_no_db() {
        expect_panic(
            || {
                Hegel::new(|tc: TestCase| {
                    let charges = pool(&tc);
                    hegel::stateful::run(DepthMachine { charges }, tc);
                })
                .settings(Settings::new().database(None).test_cases(1000))
                .run();
            },
            "depth .* is not less than 3",
        );
    }

    struct TrickyInitMachine {
        a: i64,
    }

    #[hegel::state_machine]
    impl TrickyInitMachine {
        #[rule]
        fn inc(&mut self, _tc: TestCase) {
            self.a += 1;
        }

        #[invariant]
        fn check_a_positive(&mut self, _tc: TestCase) {
            assert!(self.a >= 0, "a must be non-negative");
        }
    }

    #[test]
    fn test_invariants_are_checked_after_init_steps() {
        Hegel::new(|tc: TestCase| {
            hegel::stateful::run(TrickyInitMachine { a: 0 }, tc);
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }

    const FLAKY_MACHINE_CODE: &str = r#"
use std::sync::atomic::{AtomicBool, Ordering};
use hegel::TestCase;

static WILL_FAIL: AtomicBool = AtomicBool::new(true);

struct FlakyStateMachine;

#[hegel::state_machine]
impl FlakyStateMachine {
    #[rule]
    fn action(&mut self, _tc: TestCase) {
        // First call: swap true→false, should_fail=true → assertion fires.
        // All subsequent calls: should_fail=false → passes.
        let should_fail = WILL_FAIL.swap(false, Ordering::SeqCst);
        assert!(!should_fail, "flaky: fails on first invocation only");
    }
}

#[hegel::test(database = None)]
fn test_flaky_state_machine(tc: TestCase) {
    hegel::stateful::run(FlakyStateMachine, tc);
}

fn main() {}
"#;

    #[test]
    fn test_flaky_raises_flaky() {
        TempRustProject::new()
            .main_file(FLAKY_MACHINE_CODE)
            .expect_failure("Flaky test detected")
            .cargo_test(&[]);
    }

    struct NoRulesMachine;

    impl StateMachine for NoRulesMachine {
        fn rules(&self) -> Vec<Rule<Self>> {
            vec![]
        }
        fn invariants(&self) -> Vec<Rule<Self>> {
            vec![]
        }
    }

    #[test]
    fn test_machine_with_no_rules_is_a_usage_error() {
        expect_panic(
            || {
                Hegel::new(|tc: TestCase| {
                    hegel::stateful::run(NoRulesMachine, tc);
                })
                .settings(Settings::new().database(None))
                .run();
            },
            "cannot run a state machine with no rules",
        );
    }

    /// Records which rule ran at each step, one sequence per test case.
    struct SwarmRecorderMachine {
        runs: Arc<Mutex<Vec<Vec<usize>>>>,
    }

    impl SwarmRecorderMachine {
        fn record(&self, index: usize) {
            self.runs.lock().unwrap().last_mut().unwrap().push(index);
        }
    }

    impl StateMachine for SwarmRecorderMachine {
        fn rules(&self) -> Vec<Rule<Self>> {
            vec![
                Rule::new("rule_0", |m, _tc| m.record(0)),
                Rule::new("rule_1", |m, _tc| m.record(1)),
                Rule::new("rule_2", |m, _tc| m.record(2)),
            ]
        }
        fn invariants(&self) -> Vec<Rule<Self>> {
            vec![]
        }
    }

    /// Length of the longest run of identical consecutive elements.
    fn longest_run(sequence: &[usize]) -> usize {
        let mut longest = 0;
        let mut current = 0;
        let mut previous = None;
        for &value in sequence {
            current = if previous == Some(value) {
                current + 1
            } else {
                1
            };
            previous = Some(value);
            longest = longest.max(current);
        }
        longest
    }

    /// Swarm testing disables a subset of rules per test case, so some test
    /// cases run the same rule many times in a row. With three rules and
    /// uniform selection, a run of 20 identical rules is vanishingly unlikely
    /// ((1/3)^19 per starting point) — only the all-minimal test case (every
    /// draw 0) produces one. With swarm testing long runs are common:
    /// whenever the feature flags leave a single rule enabled, every step
    /// picks that survivor. So we assert on the *number* of test cases with a
    /// long run, not merely its existence.
    #[test]
    fn test_swarm_produces_long_runs_of_one_rule() {
        let runs: Arc<Mutex<Vec<Vec<usize>>>> = Arc::new(Mutex::new(Vec::new()));
        let runs_in_test = Arc::clone(&runs);
        Hegel::new(move |tc: TestCase| {
            runs_in_test.lock().unwrap().push(Vec::new());
            let m = SwarmRecorderMachine {
                runs: Arc::clone(&runs_in_test),
            };
            hegel::stateful::run(m, tc);
        })
        .settings(
            Settings::new()
                .test_cases(100)
                .database(None)
                .derandomize(true),
        )
        .run();

        let runs = runs.lock().unwrap();
        let long_run_count = runs.iter().filter(|s| longest_run(s) >= 20).count();
        assert!(
            long_run_count >= 10,
            "expected at least 10 of {} test cases to have a run of >= 20 \
             identical rules under swarm selection, got {long_run_count}",
            runs.len()
        );
    }

    /// Counts per test case how many times its single rule ran.
    struct StepRecorderMachine {
        counts: Arc<Mutex<Vec<u64>>>,
        fail_assumption: bool,
    }

    impl StateMachine for StepRecorderMachine {
        fn rules(&self) -> Vec<Rule<Self>> {
            vec![Rule::new("step", |m: &mut StepRecorderMachine, tc| {
                *m.counts.lock().unwrap().last_mut().unwrap() += 1;
                tc.assume(!m.fail_assumption);
            })]
        }
        fn invariants(&self) -> Vec<Rule<Self>> {
            vec![]
        }
    }

    fn run_step_recorder(fail_assumption: bool) -> Vec<u64> {
        let counts: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let counts_in_test = Arc::clone(&counts);
        Hegel::new(move |tc: TestCase| {
            counts_in_test.lock().unwrap().push(0);
            let m = StepRecorderMachine {
                counts: Arc::clone(&counts_in_test),
                fail_assumption,
            };
            hegel::stateful::run(m, tc);
        })
        .settings(
            Settings::new()
                .test_cases(100)
                .database(None)
                .derandomize(true),
        )
        .run();
        let counts = counts.lock().unwrap();
        counts.clone()
    }

    /// The engine owns the step cap: no test case runs more than 50 steps,
    /// and the unbounded cap draw usually truncates to exactly 50.
    #[test]
    fn test_step_cap_is_50_most_of_the_time() {
        let counts = run_step_recorder(false);
        assert!(counts.iter().all(|&c| c <= 50));
        let full = counts.iter().filter(|&&c| c == 50).count();
        assert!(
            full > counts.len() / 2,
            "expected most of {} test cases to run exactly 50 steps, got {full}",
            counts.len()
        );
    }

    /// The step cap counts attempted rules, not successful ones, so even a
    /// machine whose rules never get past their assumptions is bounded by
    /// the engine's cap rather than retrying indefinitely.
    #[test]
    fn test_hopeless_machine_is_bounded_by_the_step_cap() {
        let counts = run_step_recorder(true);
        assert!(counts.iter().all(|&c| c <= 50));
        let full = counts.iter().filter(|&&c| c == 50).count();
        assert!(
            full > counts.len() / 2,
            "expected most of {} test cases to attempt exactly 50 rules, got {full}",
            counts.len()
        );
    }
}
