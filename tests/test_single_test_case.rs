mod common;

use common::project::TempRustProject;
use common::utils::expect_panic;
use hegel::generators as gs;
use hegel::{Mode, TestCase};

#[test]
fn test_single_test_case_runs_exactly_one_case() {
    let mut count = 0;

    hegel::Hegel::new(|tc| {
        tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
    .run();

    assert_eq!(count, 1);
}

#[test]
fn test_single_test_case_passing() {
    hegel::Hegel::new(|tc| {
        tc.draw(gs::booleans());
    })
    .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
    .run();
}

#[test]
fn test_single_test_case_failing_propagates() {
    expect_panic(
        || {
            hegel::Hegel::new(|_tc| {
                panic!("deliberate failure");
            })
            .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
            .run();
        },
        "deliberate failure",
    );
}

#[test]
fn test_single_test_case_no_shrinking() {
    let mut count = 0;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        hegel::Hegel::new(|tc| {
            tc.draw(gs::integers::<i32>());
            count += 1;
            panic!("always fails");
        })
        .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
        .run();
    }));

    assert!(result.is_err());
    assert_eq!(count, 1);
}

#[test]
fn test_single_test_case_with_seed_is_deterministic() {
    let mut values = Vec::new();

    for _ in 0..3 {
        let mut value = 0i32;
        hegel::Hegel::new(|tc| {
            value = tc.draw(gs::integers::<i32>());
        })
        .settings(
            hegel::Settings::new()
                .mode(Mode::SingleTestCase)
                .seed(Some(42)),
        )
        .run();
        values.push(value);
    }

    assert_eq!(values[0], values[1]);
    assert_eq!(values[1], values[2]);
}

#[test]
fn test_single_test_case_assume_produces_invalid() {
    hegel::Hegel::new(|tc| {
        let n = tc.draw(gs::integers::<i32>());
        tc.assume(n > 0);
    })
    .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
    .run();
}

#[test]
fn test_single_test_case_generation_works() {
    hegel::Hegel::new(|tc| {
        let v: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()));
        let _ = v.len();
    })
    .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
    .run();
}

#[test]
fn test_single_test_case_debug_verbosity() {
    hegel::Hegel::new(|tc| {
        tc.draw(gs::booleans());
    })
    .settings(
        hegel::Settings::new()
            .mode(Mode::SingleTestCase)
            .verbosity(hegel::Verbosity::Debug),
    )
    .run();
}

#[hegel::test(mode = Mode::SingleTestCase)]
fn test_single_test_case_via_test_macro(tc: TestCase) {
    tc.draw(gs::booleans());
}

#[test]
fn test_single_test_case_repeat_loops_indefinitely() {
    let iteration_count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counter = iteration_count.clone();

    expect_panic(
        || {
            hegel::Hegel::new(move |tc| {
                tc.repeat(|| {
                    let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if n >= 100 {
                        panic!("reached 100 iterations");
                    }
                });
            })
            .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
            .run();
        },
        "reached 100 iterations",
    );

    assert_eq!(
        iteration_count.load(std::sync::atomic::Ordering::Relaxed),
        100
    );
}

#[test]
fn test_single_test_case_stateful_runs_forever() {
    let step_count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counter = step_count.clone();

    struct Counter {
        count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    #[hegel::state_machine]
    impl Counter {
        #[rule]
        fn step(&mut self, _tc: TestCase) {
            let n = self
                .count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            if n >= 200 {
                panic!("reached 200 steps");
            }
        }
    }

    expect_panic(
        move || {
            hegel::Hegel::new(move |tc| {
                let m = Counter {
                    count: counter.clone(),
                };
                hegel::stateful::run(m, tc);
            })
            .settings(hegel::Settings::new().mode(Mode::SingleTestCase))
            .run();
        },
        "reached 200 steps",
    );

    assert_eq!(
        step_count.load(std::sync::atomic::Ordering::Relaxed),
        200
    );
}

#[test]
fn test_hegel_main_macro_with_single_test_case() {
    let code = r#"
use hegel::generators as gs;

#[hegel::main]
fn main(tc: hegel::TestCase) {
    tc.draw(gs::booleans());
}
"#;
    TempRustProject::new()
        .main_file(code)
        .cargo_run(&["--", "--single-test-case"]);
}

#[test]
fn test_hegel_main_macro_without_args() {
    let code = r#"
use hegel::generators as gs;

#[hegel::main]
fn main(tc: hegel::TestCase) {
    tc.draw(gs::booleans());
}
"#;
    TempRustProject::new().main_file(code).cargo_run(&[]);
}
