//! `HEGEL_STATEFUL_STEPS` replaces every machine's step count for the run.
//! Environment variables are process-wide, so this file is its own test
//! binary and its tests take a lock around setting the variable.

use std::sync::{Arc, Mutex};

use hegel::stateful::{STEP_COUNT_VAR, machine};
use hegel::{Hegel, Settings, TestCase};

static ENV: Mutex<()> = Mutex::new(());

struct Recording {
    steps: u32,
    index: Option<usize>,
    sink: Arc<Mutex<Vec<u32>>>,
}

#[hegel::state_machine]
impl Recording {
    #[rule]
    fn step(&mut self, _: TestCase) {
        self.steps += 1;
        let mut sink = self.sink.lock().unwrap();
        match self.index {
            Some(i) => sink[i] = self.steps,
            None => {
                sink.push(self.steps);
                self.index = Some(sink.len() - 1);
            }
        }
    }
}

fn longest_run_with(value: &str, configured: i64) -> u32 {
    let _guard = ENV.lock().unwrap();
    unsafe { std::env::set_var(STEP_COUNT_VAR, value) };
    let sink: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = sink.clone();
    Hegel::new(move |tc: TestCase| {
        machine(Recording {
            steps: 0,
            index: None,
            sink: seen.clone(),
        })
        .steps(configured)
        .run(tc);
    })
    .settings(Settings::new().test_cases(30).database(None))
    .run();
    unsafe { std::env::remove_var(STEP_COUNT_VAR) };
    let recorded = sink.lock().unwrap();
    recorded.iter().copied().max().unwrap()
}

#[test]
fn the_variable_replaces_the_configured_step_count() {
    assert_eq!(longest_run_with("7", 3), 7);
}

#[test]
fn an_empty_variable_leaves_the_configured_step_count() {
    assert_eq!(longest_run_with("", 3), 3);
}

#[test]
fn a_value_that_is_not_a_positive_integer_is_an_error() {
    let _guard = ENV.lock().unwrap();
    unsafe { std::env::set_var(STEP_COUNT_VAR, "lots") };
    let result = std::panic::catch_unwind(|| {
        Hegel::new(|tc: TestCase| {
            machine(Recording {
                steps: 0,
                index: None,
                sink: Arc::new(Mutex::new(Vec::new())),
            })
            .run(tc);
        })
        .settings(Settings::new().test_cases(1).database(None))
        .run();
    });
    unsafe { std::env::remove_var(STEP_COUNT_VAR) };
    assert!(result.is_err());
}
