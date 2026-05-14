mod common;

use common::project::TempRustProject;

fn read_values(dir: &std::path::Path, label: &str) -> Vec<i64> {
    let path = dir.join(label);
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect()
}

#[test]
fn test_database_key_replays_failure() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("database");
    std::fs::create_dir_all(&db_path).unwrap();
    // Use forward slashes to avoid invalid escape sequences in generated Rust string literals
    let db_str = db_path.to_str().unwrap().replace('\\', "/");

    let test_code = format!(
        r#"
use hegel::generators as gs;
use std::io::Write;

fn record_test_case(label: &str, n: i64) {{
    let path = format!("{{}}/{{}}", std::env::var("VALUES_DIR").unwrap(), label);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{{}}", n).unwrap();
}}

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_1(tc: hegel::TestCase) {{
    let n: i64 = tc.draw(gs::integers());
    record_test_case("test_1", n);
    assert!(n < 1_000_000);
}}

#[hegel::test(database = Some("{db_str}".to_string()))]
fn test_2(tc: hegel::TestCase) {{
    let n: i64 = tc.draw(gs::integers());
    record_test_case("test_2", n);
    assert!(n < 1_000_000);
}}
"#
    );

    let values_path = temp_dir.path().join("values");
    std::fs::create_dir_all(&values_path).unwrap();
    let project = TempRustProject::new()
        .test_file("integration.rs", &test_code)
        .env("VALUES_DIR", values_path.to_str().unwrap())
        // "FAILED" appears in cargo test output for server backends.
        .expect_failure("FAILED");

    // run test_1. Database now has a failing entry for test_1
    project.cargo_test(&["test_1"]);

    let shrunk_value = *read_values(&values_path, "test_1").last().unwrap();
    assert_eq!(shrunk_value, 1_000_000);

    // clear the log file
    std::fs::remove_file(values_path.join("test_1")).unwrap();

    // run test_1 again. It should replay the shrunk test case immediately
    project.cargo_test(&["test_1"]);

    let values = read_values(&values_path, "test_1");
    assert_eq!(
        values[0], shrunk_value,
        "Expected to replay shrunk test case {shrunk_value} first, got {}",
        values[0]
    );

    // run test_2. It should not replay the test_1 shrunk test case.
    project.cargo_test(&["test_2"]);

    let values = read_values(&values_path, "test_2");
    assert_ne!(values[0], shrunk_value);
}

mod replay_logic {
    //! `test_does_not_shrink_on_replay_with_multiple_bugs` is skipped:
    //! it depends on `report_multiple_bugs=True` and Python's `ExceptionGroup`,
    //! neither of which has a counterpart in hegel-rust.

    use std::sync::{Arc, Mutex};

    use hegel::generators as gs;
    use hegel::{Hegel, Settings, TestCase};

    fn run_expecting_failure<F>(f: F)
    where
        F: FnOnce() + std::panic::UnwindSafe,
    {
        let result = std::panic::catch_unwind(f);
        assert!(result.is_err(), "expected the test to fail");
    }

    #[test]
    fn test_does_not_shrink_on_replay() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap().to_string();

        let call_count: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
        let is_first: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
        let last: Arc<Mutex<Option<Vec<i64>>>> = Arc::new(Mutex::new(None));

        let run = || {
            let call_count = Arc::clone(&call_count);
            let is_first = Arc::clone(&is_first);
            let last = Arc::clone(&last);
            let db_path = db_path.clone();
            run_expecting_failure(std::panic::AssertUnwindSafe(move || {
                Hegel::new(move |tc: TestCase| {
                    let ls: Vec<i64> =
                        tc.draw(gs::vecs(gs::integers::<i64>()).min_size(3).unique(true));
                    {
                        let mut first = is_first.lock().unwrap();
                        let mut last = last.lock().unwrap();
                        if *first && last.is_some() {
                            assert_eq!(&ls, last.as_ref().unwrap());
                        }
                        *first = false;
                        *last = Some(ls);
                    }
                    *call_count.lock().unwrap() += 1;
                    panic!("boom");
                })
                .settings(
                    Settings::new()
                        .database(Some(db_path))
                        .derandomize(false)
                        .test_cases(1000),
                )
                .__database_key("test_does_not_shrink_on_replay".to_string())
                .run();
            }));
        };

        run();
        assert!(last.lock().unwrap().is_some());

        *call_count.lock().unwrap() = 0;
        *is_first.lock().unwrap() = true;

        run();
        assert_eq!(*call_count.lock().unwrap(), 2);
    }

    #[test]
    fn test_will_always_shrink_if_previous_example_does_not_replay() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap().to_string();

        let good: Arc<Mutex<std::collections::HashSet<i64>>> =
            Arc::new(Mutex::new(std::collections::HashSet::new()));
        let last: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));

        for i in 0..20 {
            let good_cl = Arc::clone(&good);
            let last_cl = Arc::clone(&last);
            let db_path_cl = db_path.clone();
            run_expecting_failure(std::panic::AssertUnwindSafe(move || {
                Hegel::new(move |tc: TestCase| {
                    let n: i64 = tc.draw(gs::integers::<i64>().min_value(0));
                    if !good_cl.lock().unwrap().contains(&n) {
                        *last_cl.lock().unwrap() = Some(n);
                        panic!("boom");
                    }
                })
                .settings(
                    Settings::new()
                        .database(Some(db_path_cl))
                        .derandomize(false)
                        .test_cases(1000),
                )
                .__database_key("test_will_always_shrink".to_string())
                .run();
            }));
            assert_eq!(*last.lock().unwrap(), Some(i));
            good.lock().unwrap().insert(last.lock().unwrap().unwrap());
        }
    }

    #[test]
    fn test_will_shrink_if_the_previous_example_does_not_look_right() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap().to_string();

        let last: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));
        let first_test: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));

        let run = || {
            let last = Arc::clone(&last);
            let first_test = Arc::clone(&first_test);
            let db_path = db_path.clone();
            run_expecting_failure(std::panic::AssertUnwindSafe(move || {
                Hegel::new(move |tc: TestCase| {
                    let m: i64 = tc.draw(gs::integers::<i64>());
                    *last.lock().unwrap() = Some(m);
                    if *first_test.lock().unwrap() {
                        tc.draw(gs::integers::<i64>());
                        assert!(m < 10000);
                    } else {
                        panic!("boom");
                    }
                })
                .settings(Settings::new().database(Some(db_path)).derandomize(false))
                .__database_key("test_will_shrink_misaligned".to_string())
                .run();
            }));
        };

        run();
        let val = *last.lock().unwrap();
        assert!(val.is_some());
        assert!(val.unwrap() > 0);

        *first_test.lock().unwrap() = false;
        run();
        assert_eq!(*last.lock().unwrap(), Some(0));
    }
}
