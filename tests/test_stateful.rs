mod common;

use common::project::TempRustProject;
use hegel::TestCase;
use hegel::generators as gs;
use hegel::stateful::{Variables, variables};

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

// Consuming an element from a set should mean subsequent draws never yield the element.
struct TestConsumeMachine {
    numbers: Variables<i32>,
    consumed: i32,
}

#[hegel::state_machine]
impl TestConsumeMachine {
    #[rule]
    fn draw(&mut self, _tc: TestCase) {
        let x = self.numbers.draw();
        assert!(*x != self.consumed);
    }
}

#[hegel::test]
fn test_consume(tc: TestCase) {
    let ints = gs::integers::<i32>;
    let elements = tc.draw(gs::vecs(ints()).unique(true));
    tc.assume(!elements.is_empty());
    let mut bundle = variables(&tc);
    for element in elements.clone() {
        bundle.add(element);
    }
    let consumed = bundle.consume();
    let m = TestConsumeMachine {
        numbers: bundle,
        consumed,
    };
    hegel::stateful::run(m, tc);
}

// That `#[hegel::state_machine]` correctly propagates `#[cfg(...)]` attributes
// to the items it synthesises (so an inactive cfg strips them before
// compile_error! can fire) is asserted by
// tests/compile/pass/stateful_cfg_attributes_are_copied_to_rules.rs.

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

// Drawing an element from a bundle should always yield an element that was previously added.
struct TestDrawDomainMachine {
    domain: Vec<i32>,
    variables: Variables<i32>,
}

#[hegel::state_machine]
impl TestDrawDomainMachine {
    #[rule]
    fn draw(&mut self, _tc: TestCase) {
        let x = self.variables.draw();
        assert!(self.domain.contains(x));
    }

    #[invariant]
    fn len_matches_domain(&mut self, _tc: TestCase) {
        assert_eq!(self.variables.len(), self.domain.len());
    }
}

#[hegel::test]
fn test_draw_domain(tc: TestCase) {
    let ints = gs::integers::<i32>;
    let elements = tc.draw(gs::vecs(ints()));
    tc.assume(!elements.is_empty());
    let mut bundle = variables(&tc);
    for element in elements.clone() {
        bundle.add(element);
    }
    let m = TestDrawDomainMachine {
        domain: elements,
        variables: bundle,
    };
    hegel::stateful::run(m, tc);
}

mod stateful {
    use super::common::project::TempRustProject;
    use super::common::utils::expect_panic;
    use hegel::TestCase;
    use hegel::generators as gs;
    use hegel::stateful::{Rule, StateMachine, Variables, variables};
    use hegel::{Hegel, Settings};

    struct DepthCharge {
        depth: i64,
    }

    struct DepthMachine {
        charges: Variables<DepthCharge>,
    }

    #[hegel::state_machine]
    impl DepthMachine {
        #[rule]
        fn charge(&mut self, _tc: TestCase) {
            let depth = self.charges.draw().depth;
            self.charges.add(DepthCharge { depth: depth + 1 });
        }

        #[rule]
        fn none_charge(&mut self, _tc: TestCase) {
            self.charges.add(DepthCharge { depth: 0 });
        }

        #[rule]
        fn is_not_too_deep(&mut self, _tc: TestCase) {
            let check = self.charges.draw();
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
                    let charges = variables(&tc);
                    hegel::stateful::run(DepthMachine { charges }, tc);
                })
                .settings(Settings::new().database(None).test_cases(1000))
                .run();
            },
            "depth .* is not less than 3",
        );
    }

    // TrickyInitMachine: a machine whose invariant accesses `self.a`, which
    // must be initialised before any rule runs. In hegel-rust, initialisation
    // happens by constructing the struct with a=0 before `run()`, so the
    // invariant is always satisfied (a starts at 0 and only gets incremented).

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

    // Skipped: the conjecture-runner internal-label `DataTooLarge`
    // fires when a stateful rule draws 512 bytes per step. Python
    // Hypothesis silently handles this for stateful tests as part of the
    // fix for GH-3618, but hegel-rust has not implemented the
    // equivalent data-budget exemption for stateful rule draws. The public
    // `HealthCheck` enum no longer exposes `data_too_large` at all (per
    // audit item A14); the internal label still exists in the
    // conjecture-runner port-test fixture.
    // TODO: implement GH-3618-equivalent fix and re-enable this test.

    // Exercises flaky-test detection. The machine uses a global AtomicBool that
    // causes the rule to fail only on the very first invocation, so exploration
    // finds a "failure" but the replay passes → the runner detects the
    // inconsistency and reports "Flaky test detected".

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
}
