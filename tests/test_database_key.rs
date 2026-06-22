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
        // "FAILED" appears in the cargo test output of the failing inner test.
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

    /// Recursively count regular files under `dir`. Returns 0 if `dir`
    /// does not exist. Used by the persistence tests below to check
    /// whether the database has any stored entries without needing
    /// access to internal hashing.
    fn db_file_count(dir: &std::path::Path) -> usize {
        let entries = match std::fs::read_dir(dir) {
            Ok(d) => d,
            Err(_) => return 0,
        };
        let mut n = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                n += db_file_count(&path);
            } else {
                n += 1;
            }
        }
        n
    }

    /// Regression test: a failing test case must be persisted to the
    /// database as soon as it is discovered, not only at the end of the
    /// run. If the runner is killed (e.g. Ctrl+C during shrinking), the
    /// initially-found failure — and any intermediate shrunk version —
    /// must already be saved.
    ///
    /// This test exercises the discovery save: from inside the test
    /// body, we observe the database directory. By the time the body is
    /// invoked again (during shrinking), the previously-discovered
    /// failing example must already be on disk.
    #[test]
    fn test_failure_persisted_immediately_when_discovered() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap().to_string();
        let db_path_for_check = db_path.clone();

        // Earliest call number (1-indexed) on which the test body
        // observed at least one entry in the database. `None` means the
        // body never saw a populated database during the run.
        let observed_at: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
        let observed_cl = Arc::clone(&observed_at);
        let calls: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
        let calls_cl = Arc::clone(&calls);
        let db_root = std::path::PathBuf::from(&db_path_for_check);

        run_expecting_failure(std::panic::AssertUnwindSafe(move || {
            Hegel::new(move |tc: TestCase| {
                {
                    let mut c = calls_cl.lock().unwrap();
                    *c += 1;
                    if observed_cl.lock().unwrap().is_none() && db_file_count(&db_root) > 0 {
                        *observed_cl.lock().unwrap() = Some(*c);
                    }
                }
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0));
                assert!(n < 1_000_000);
            })
            .settings(
                Settings::new()
                    .database(Some(db_path))
                    .derandomize(false)
                    .test_cases(1000),
            )
            .__database_key("test_failure_persisted_immediately_when_discovered".to_string())
            .run();
        }));

        // Sanity check: the run actually performed more than one test case.
        // (A failing test with shrinking will run many more.)
        let total_calls = *calls.lock().unwrap();
        assert!(
            total_calls > 1,
            "expected the run to make more than one test-case call, got {total_calls}"
        );

        // The database must have entries STRICTLY BEFORE the final test
        // body call. If `observed_at` is `None` (db never populated) or
        // equals `total_calls` (db only populated for the final replay),
        // persistence happens too late and killing mid-shrink loses the
        // failure.
        let observed = *observed_at.lock().unwrap();
        assert!(
            observed.is_some_and(|o| o < total_calls),
            "Database was not populated during the run \
             (observed_at={observed:?}, total_calls={total_calls}). \
             Killing the runner mid-shrink would lose the failure."
        );
    }

    /// Stronger regression test: intermediate shrunk versions must
    /// reach the database before the next test-body call. We snapshot
    /// the DB file count on every body invocation; if the shrinker is
    /// improving the DB incrementally, the count should change between
    /// the first observation (single discovered failure) and the last
    /// (potentially the smallest shrunk version). If saves are batched
    /// to end-of-run, observed snapshots would either all be 0 or all
    /// jump from 0 to N on the final replay alone.
    #[test]
    fn test_intermediate_shrinks_update_db_during_run() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap().to_string();
        let db_root = std::path::PathBuf::from(&db_path);
        let db_root_cl = db_root.clone();

        // Distinct (file-count, total-bytes) snapshots observed by the
        // body across all of its calls. A working incremental persister
        // shows at least two distinct snapshots (the DB grows / shrinks
        // as the shrinker makes progress); a broken one shows just
        // `{(0, 0)}` (DB empty during the run) or a single non-empty
        // snapshot reached only at the very end.
        let snapshots: Arc<Mutex<std::collections::HashSet<(usize, usize)>>> =
            Arc::new(Mutex::new(std::collections::HashSet::new()));
        let snapshots_cl = Arc::clone(&snapshots);
        let calls: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
        let calls_cl = Arc::clone(&calls);

        // Boundary chosen so the minimal counterexample (n =
        // BOUNDARY, v = []) cannot be drawn directly by random
        // generation. The native integer sampler heavily biases toward
        // "nasty" constants (powers of two/ten, factorials, etc. plus
        // ±1 neighbours); 1_234_567 is in none of those sets, so the
        // shrinker is guaranteed to have multi-step work to do
        // converging on it — and therefore guaranteed to emit
        // intermediate persister saves.
        const BOUNDARY: i64 = 1_234_567;

        run_expecting_failure(std::panic::AssertUnwindSafe(move || {
            Hegel::new(move |tc: TestCase| {
                {
                    *calls_cl.lock().unwrap() += 1;
                    let (count, total_bytes) = db_summary(&db_root_cl);
                    snapshots_cl.lock().unwrap().insert((count, total_bytes));
                }
                let n: i64 = tc.draw(gs::integers::<i64>().min_value(0));
                let v: Vec<i64> = tc.draw(gs::vecs(gs::integers::<i64>()).min_size(0));
                let _ = v;
                assert!(n < BOUNDARY);
            })
            .settings(
                Settings::new()
                    .database(Some(db_path))
                    .derandomize(false)
                    .test_cases(1000),
            )
            .__database_key("test_intermediate_shrinks_update_db_during_run".to_string())
            .run();
        }));

        let snaps = snapshots.lock().unwrap().clone();
        // Reached the body at least a few times.
        let total = *calls.lock().unwrap();
        assert!(total > 1, "expected multiple test-body calls, got {total}");

        // The DB went through multiple distinct *non-empty* states during
        // the run — i.e. the shrinker's improvements were persisted as
        // they happened, not collapsed into a single end-of-run write.
        // Without incremental persistence, the body sees only `(0, 0)`
        // throughout shrinking and a single end-state on the final
        // replay → at most one non-empty snapshot.
        let distinct_non_empty = snaps.iter().filter(|&&(_, b)| b > 0).count();
        assert!(
            distinct_non_empty >= 2,
            "Only saw {distinct_non_empty} distinct non-empty DB \
             state(s) across {total} test-body calls (snapshots: \
             {snaps:?}). The shrinker is not persisting intermediate \
             improvements — killing it mid-shrink would lose progress."
        );
    }

    /// Recursively compute `(file_count, total_bytes)` for everything
    /// under `root`. Used by the persistence tests to summarise the
    /// database state without depending on the internal hashing.
    fn db_summary(root: &std::path::Path) -> (usize, usize) {
        let entries = match std::fs::read_dir(root) {
            Ok(d) => d,
            Err(_) => return (0, 0),
        };
        let mut count = 0;
        let mut total = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let (c, t) = db_summary(&path);
                count += c;
                total += t;
            } else if let Ok(meta) = std::fs::metadata(&path) {
                count += 1;
                total += meta.len() as usize;
            }
        }
        (count, total)
    }
}
