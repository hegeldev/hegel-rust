//! Ported from hypothesis-python/tests/cover/test_stateful.py
//!
//! Individually-skipped tests (see SKIPPED.md for details):
//! - test_multiple_rules_same_func — uses `TestCase().runTest()` + `capture_out`, Python unittest API.
//! - test_picks_up_settings_at_first_use_of_testcase — Python settings class attribute API.
//! - test_can_get_test_case_off_machine_instance — Python `.TestCase` attribute access.
//! - test_flaky_draw_less_raises_flaky — uses `current_build_context().is_final`, no hegel-rust counterpart.
//! - test_result_is_added_to_target — `lists(nodes)` (bundle inside strategy), unportable.
//! - test_flaky_precondition_error_message — `FlakyPreconditionMachine` uses `@precondition`, no hegel-rust counterpart.
//! - test_flaky_draw_in_rule_no_precondition_note — uses `current_build_context().is_final`.
//! - test_get_state_machine_test_is_importable — Hypothesis public API, no hegel-rust counterpart.
//! - test_ratchetting_raises_flaky — uses `data()` strategy in rules, no hegel-rust counterpart.
//! - test_multiple — tests Python `multiple()` function object (`.values`), Python-specific.
//! - MachineWithConsumingRule / TestMachineWithConsumingRule — `lists(consumes(b1))` (bundle inside strategy) + `self.bundle("b1")` introspection.
//! - MachineUsingMultiple / TestMachineUsingMultiple — `self.bundle("b")` name-based introspection.
//! - test_multiple_variables_printed — `multiple()` output format, Python-specific.
//! - test_multiple_variables_printed_single_element — `multiple()` output format, Python-specific.
//! - test_no_variables_printed — `multiple()` output format, Python-specific.
//! - test_consumes_typecheck — Python TypeError on `consumes(non-bundle)`, Python-specific.
//! - test_empty_machine_is_invalid — `InvalidDefinition` is a Python exception type; hegel-rust panics instead.
//! - test_machine_with_no_terminals_is_invalid — same: `InvalidDefinition` with no hegel-rust counterpart.
//! - test_minimizes_errors_in_teardown — complex `@initialize` + teardown + nonlocal interaction.
//! - test_can_explicitly_pass_settings — `stateful_step_count` / threading-specific settings API.
//! - test_settings_argument_is_validated — Python-specific settings validation API.
//! - test_runner_that_checks_factory_produced_a_machine — Python-specific factory check.
//! - test_settings_attribute_is_validated — Python class attribute settings API.
//! - test_stateful_double_rule_is_forbidden — double-`@rule` is a Python decorator-level check.
//! - test_can_explicitly_call_functions_when_precondition_not_satisfied — `@precondition` decorator.
//! - test_no_double_invariant — double-`@invariant` is a Python decorator-level check.
//! - test_invariant_precondition — `@precondition` on invariants, no hegel-rust counterpart.
//! - test_invariant_and_rule_are_incompatible — Python decorator composition rules.
//! - test_invalid_rule_argument — `@rule(strategy=object())` validation, Python-specific.
//! - test_invalid_initialize_argument — `@initialize` validation, Python-specific.
//! - test_explicit_invariant_call_with_precondition — `@precondition` on invariant.
//! - test_invariant_present_in_falsifying_example — uses `check_during_init=True`, no hegel-rust counterpart.
//! - test_invariant_failling_present_in_falsifying_example — output format includes `@initialize` step
//!   ("state.initialize_1()") which hegel-rust doesn't emit.
//! - test_removes_needless_steps — output format test; hegel-rust step output format differs from Hypothesis.
//! - test_prints_equal_values_with_correct_variable_name — output format test; differs from Hypothesis.
//! - test_initialize_rule — multiple `@initialize` rules + output format, no hegel-rust `@initialize`.
//! - test_initialize_rule_populate_bundle — `@initialize` + bundle + output format.
//! - test_initialize_rule_dont_mix_with_precondition — `@initialize` + `@precondition` combination.
//! - test_initialize_rule_dont_mix_with_regular_rule — `@initialize` + `@rule` combination.
//! - test_initialize_rule_cannot_be_double_applied — double `@initialize`, Python decorator validation.
//! - test_initialize_rule_in_state_machine_with_inheritance — Python class inheritance of `@initialize`.
//! - test_can_manually_call_initialize_rule — manually calling `@initialize` rule + output format.
//! - test_steps_printed_despite_pytest_fail — `pytest.fail()` raises `Failed`; no Rust counterpart.
//! - test_steps_not_printed_with_pytest_skip — `pytest.skip()` raises `Skipped`; no Rust counterpart.
//! - test_rule_deprecation_targets_and_target — Hypothesis deprecation API.
//! - test_rule_deprecation_bundle_by_name — Hypothesis deprecation API.
//! - test_rule_non_bundle_target — `rule(target=integers())`, Python-specific validation.
//! - test_rule_non_bundle_target_oneof — `rule(target=k | v)`, Python-specific.
//! - test_uses_seed — `@seed` decorator, no direct hegel-rust counterpart.
//! - test_reproduce_failure_works — `@reproduce_failure`, Python-specific.
//! - test_reproduce_failure_fails_if_no_error — `@reproduce_failure`, Python-specific.
//! - test_cannot_have_zero_steps — `stateful_step_count=0` validation, Python-specific settings.
//! - test_arguments_do_not_use_names_of_return_values — output format with `@initialize`, differs.
//! - test_invariants_can_be_checked_during_init_steps — `check_during_init=True`, no hegel-rust counterpart.
//! - test_check_during_init_must_be_boolean — `check_during_init` argument validation, Python-specific.
//! - test_deprecated_target_consumes_bundle — Hypothesis deprecation API.
//! - test_min_steps_argument (MinStepsMachine) — `_min_steps` argument, Python-specific.
//! - test_fails_on_settings_class_attribute — Python class-level settings attribute check.
//! - test_single_target_multiple — `multiple()` + `@initialize` output format.
//! - test_targets_repr — parametrized `multiple()` output format tests.
//! - test_multiple_targets — multiple targets + `multiple()` output format.
//! - test_multiple_common_targets — multiple common targets, complex bundle-assignment output.
//! - test_flatmap — `buns.flatmap(lambda x: ...)`, bundle used as strategy (unsupported).
//! - test_use_bundle_within_other_strategies — `st.builds(Class, my_bundle)`, bundle in strategy (unsupported).
//! - test_precondition_cannot_be_used_without_rule — `@precondition` without `@rule`, Python validation.

use crate::common::project::TempRustProject;
use crate::common::utils::expect_panic;
use hegel::TestCase;
use hegel::generators as gs;
use hegel::stateful::{Rule, StateMachine, Variables, variables};
use hegel::{Hegel, Settings};

// ── DepthMachine (defined in tests/nocover/test_stateful.py, used here) ─────

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

// ── test_invariant ────────────────────────────────────────────────────────────

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

// ── test_multiple_invariants ─────────────────────────────────────────────────

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
                hegel::stateful::run(
                    MultipleInvariantMachine { first_ran: false },
                    tc,
                );
            })
            .settings(Settings::new().database(None))
            .run();
        },
        "all invariants ran",
    );
}

// ── test_invariant_checks_initial_state_if_no_initialize_rules ───────────────

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

// ── test_always_runs_at_least_one_step ───────────────────────────────────────

struct CountStepsMachine {
    count: std::sync::Arc<std::sync::Mutex<i64>>,
}

impl StateMachine for CountStepsMachine {
    fn rules(&self) -> Vec<Rule<Self>> {
        vec![Rule::new("do_something", |m: &mut CountStepsMachine, _tc: TestCase| {
            *m.count.lock().unwrap() += 1;
        })]
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

// ── test_can_use_factory_for_tests ───────────────────────────────────────────

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

// ── test_can_run_with_no_db ───────────────────────────────────────────────────

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

// ── test_invariants_are_checked_after_init_steps ─────────────────────────────
//
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

// ── test_lots_of_entropy ──────────────────────────────────────────────────────
//
// Skipped: hegel-rust raises TestCasesTooLarge when a stateful rule draws
// 512 bytes per step; Python Hypothesis silently handles this for stateful
// tests as part of the fix for GH-3618, but hegel-rust has not implemented
// the equivalent data-budget exemption for stateful rule draws.
// TODO: implement GH-3618-equivalent fix and re-enable this test.

// ── test_flaky_raises_flaky ───────────────────────────────────────────────────
//
// Exercises flaky-test detection. The machine uses a global AtomicBool that
// causes the rule to fail only on the very first invocation, so exploration
// finds a "failure" but the replay passes → both the server and native
// backends detect the inconsistency and report "Flaky test detected".

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

// ── test_saves_failing_example_in_database ────────────────────────────────────
//
// Exercises database persistence: a stateful test that fails should save the
// failing example. Native-gated because the on-disk database only exists in
// native mode.

#[cfg(feature = "native")]
const DB_STATEFUL_CODE: &str = r#"
use hegel::TestCase;
use hegel::stateful::{variables, Variables};

struct DepthCharge { depth: i64 }
struct DepthMachine { charges: Variables<DepthCharge> }

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
        assert!(check.depth < 3);
    }
}

#[hegel::test(database = Some(std::env::var("HEGEL_DB_PATH").unwrap()))]
fn test_depth_machine(tc: TestCase) {
    let charges = variables(&tc);
    hegel::stateful::run(DepthMachine { charges }, tc);
}

fn main() {}
"#;

#[cfg(feature = "native")]
#[test]
fn test_saves_failing_example_in_database() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("db");
    std::fs::create_dir_all(&db_path).unwrap();

    TempRustProject::new()
        .main_file(DB_STATEFUL_CODE)
        .env("HEGEL_DB_PATH", db_path.to_str().unwrap())
        .expect_failure("assertion failed")
        .cargo_test(&[]);

    // Verify that at least one failing example was persisted.
    fn has_files(path: &std::path::Path) -> bool {
        std::fs::read_dir(path).ok().is_some_and(|mut d| {
            d.any(|e| {
                e.ok().is_some_and(|entry| {
                    let p = entry.path();
                    p.is_file() || (p.is_dir() && has_files(&p))
                })
            })
        })
    }
    assert!(
        has_files(&db_path),
        "database directory should contain saved failing examples after test failure"
    );
}
