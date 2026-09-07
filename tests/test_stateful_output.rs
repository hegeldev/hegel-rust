//! Snapshot tests for the printed output of failing stateful tests.
//!
//! These pin the exact shape of a stateful counterexample: each step prints
//! as a `Step N: <rule> {` … `}` block holding that rule's draws and notes,
//! with draw names scoped to the invocation. They run in-process and capture
//! output via `hegel::with_output_override`, like `test_loop.rs`.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use hegel::TestCase;
use hegel::generators as gs;
use hegel::{Hegel, Settings};

/// Run `body` as a Hegel property test and return the lines captured during
/// the final replay of the shrunk failing case, trimmed of the failure
/// diagnostic (whose backtrace is machine-specific).
fn capture_stateful_output<F>(body: F) -> String
where
    F: FnMut(TestCase) + 'static,
{
    let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_writer = buf.clone();
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| buf_writer.lock().unwrap().push(s.to_string()));

    let _ = catch_unwind(AssertUnwindSafe(|| {
        hegel::with_output_override(sink, || {
            Hegel::new(body)
                .settings(
                    Settings::new()
                        .test_cases(200)
                        .database(None)
                        .derandomize(true),
                )
                .run();
        });
    }));

    let lines = buf.lock().unwrap().clone();
    lines
        .iter()
        .take_while(|line| !line.starts_with("thread '"))
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

struct Accumulator {
    total: i32,
}

#[hegel::state_machine]
impl Accumulator {
    #[rule]
    fn add(&mut self, tc: TestCase) {
        let n: i32 = tc.draw(gs::integers::<i32>().min_value(1).max_value(10));
        self.total += n;
    }

    #[invariant]
    fn small(&mut self, _tc: TestCase) {
        assert!(self.total < 3);
    }
}

#[test]
fn snapshot_step_labels_precede_their_rules_draws() {
    let output = capture_stateful_output(|tc: TestCase| {
        hegel::stateful::run(Accumulator { total: 0 }, tc);
    });
    insta::assert_snapshot!(output, @"
    Initial invariant check.
    Step 1: add {
      let n = 3;
    }
    ");
}

struct TwoDraws;

#[hegel::state_machine]
impl TwoDraws {
    #[rule]
    fn pair(&mut self, tc: TestCase) {
        let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
        let y: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
        assert!(x + y < 5);
    }
}

#[test]
fn snapshot_multiple_draws_stay_under_one_step_label() {
    let output = capture_stateful_output(|tc: TestCase| {
        hegel::stateful::run(TwoDraws, tc);
    });
    insta::assert_snapshot!(output, @"
    Initial invariant check.
    Step 1: pair {
      let x = 0;
      let y = 5;
    }
    ");
}

struct DrawlessSteps {
    count: i32,
}

#[hegel::state_machine]
impl DrawlessSteps {
    #[rule]
    fn bump(&mut self, _tc: TestCase) {
        self.count += 1;
    }

    #[invariant]
    fn below_three(&mut self, _tc: TestCase) {
        assert!(self.count < 3);
    }
}

#[test]
fn snapshot_drawless_steps_and_invariant_notes() {
    let output = capture_stateful_output(|tc: TestCase| {
        hegel::stateful::run(DrawlessSteps { count: 0 }, tc);
    });
    insta::assert_snapshot!(output, @"
    Initial invariant check.
    Step 1: bump {
    }
    Step 2: bump {
    }
    Step 3: bump {
    }
    ");
}

struct SlowAccumulator {
    total: i32,
}

#[hegel::state_machine]
impl SlowAccumulator {
    #[rule]
    fn add_one(&mut self, tc: TestCase) {
        let n: i32 = tc.draw(gs::integers::<i32>().min_value(1).max_value(1));
        self.total += n;
    }

    #[invariant]
    fn small(&mut self, _tc: TestCase) {
        assert!(self.total < 3);
    }
}

#[test]
fn snapshot_draw_names_reset_in_each_steps_scope() {
    let output = capture_stateful_output(|tc: TestCase| {
        hegel::stateful::run(SlowAccumulator { total: 0 }, tc);
    });
    insta::assert_snapshot!(output, @"
    Initial invariant check.
    Step 1: add_one {
      let n = 1;
    }
    Step 2: add_one {
      let n = 1;
    }
    Step 3: add_one {
      let n = 1;
    }
    ");
}

struct NotesInsideRules;

#[hegel::state_machine]
impl NotesInsideRules {
    #[rule]
    fn noted(&mut self, tc: TestCase) {
        let flag = tc.draw(gs::booleans());
        tc.note(&format!("flag was {flag}"));
        assert!(!flag);
    }
}

#[test]
fn snapshot_notes_inside_rules_follow_their_draws() {
    let output = capture_stateful_output(|tc: TestCase| {
        hegel::stateful::run(NotesInsideRules, tc);
    });
    insta::assert_snapshot!(output, @"
    Initial invariant check.
    Step 1: noted {
      let flag = true;
      flag was true
    }
    ");
}

struct HelperMethodMachine {
    total: i32,
}

#[hegel::state_machine]
impl HelperMethodMachine {
    #[hegel::test_helper]
    fn draw_amount(&self, tc: &TestCase) -> i32 {
        let negate = tc.draw(gs::booleans());
        let amount = tc.draw(gs::integers::<i32>().min_value(1).max_value(10));
        if negate { -amount } else { amount }
    }

    #[rule]
    fn add(&mut self, tc: TestCase) {
        self.total += self.draw_amount(&tc);
    }

    #[invariant]
    fn small(&mut self, _tc: TestCase) {
        assert!(self.total.unsigned_abs() < 3);
    }
}

#[test]
fn snapshot_test_helper_draws_are_named_inside_steps() {
    let output = capture_stateful_output(|tc: TestCase| {
        hegel::stateful::run(HelperMethodMachine { total: 0 }, tc);
    });
    insta::assert_snapshot!(output, @"
    Initial invariant check.
    Step 1: add {
      let negate_1 = false;
      let amount_1 = 3;
    }
    ");
}
