//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_replay_logic.py.
//!
//! `test_does_not_shrink_on_replay_with_multiple_bugs` is skipped (see
//! `SKIPPED.md`): it depends on `report_multiple_bugs=True` and Python's
//! `ExceptionGroup`, neither of which has a counterpart in hegel-rust.

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
                    tc.draw(&gs::vecs(gs::integers::<i64>()).min_size(3).unique(true));
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
                let n: i64 = tc.draw(&gs::integers::<i64>().min_value(0));
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
                let m: i64 = tc.draw(&gs::integers::<i64>());
                *last.lock().unwrap() = Some(m);
                if *first_test.lock().unwrap() {
                    tc.draw(&gs::integers::<i64>());
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
