use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Phase, Settings};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn failing_lines_with<F>(body: F, settings: Settings) -> Vec<String>
where
    F: FnMut(hegel::TestCase) + 'static,
{
    let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = buf.clone();
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));
    let result = catch_unwind(AssertUnwindSafe(|| {
        hegel::with_output_override(sink, || {
            Hegel::new(body).settings(settings).run();
        });
    }));
    assert!(result.is_err(), "expected the property to fail");
    let mut lines = buf.lock().unwrap().clone();
    if let Some(pos) = lines
        .iter()
        .position(|l| l.starts_with("thread '") && l.contains("panicked at"))
    {
        lines.truncate(pos);
    }
    lines
}

fn failing_lines<F>(body: F) -> Vec<String>
where
    F: FnMut(hegel::TestCase) + 'static,
{
    failing_lines_with(
        body,
        Settings::new()
            .test_cases(50)
            .database(None)
            .derandomize(true),
    )
}

#[test]
fn irrelevant_draws_are_annotated() {
    let lines = failing_lines(|tc| {
        let _ignored: i32 = tc.draw(gs::integers());
        let b: i32 = tc.draw(gs::integers());
        assert!(b < 0, "boom");
    });
    assert_eq!(
        lines,
        vec![
            "let draw_1 = 0;  // or any other generated value",
            "let draw_2 = 0;",
        ]
    );
}

#[test]
fn a_single_list_element_can_be_annotated() {
    let lines = failing_lines(|tc| {
        let v: Vec<i32> = tc.draw(gs::vecs(gs::integers()).min_size(3).max_size(3));
        assert!(!(v[0] >= 0 && v[2] >= 1), "boom");
    });
    assert_eq!(
        lines,
        vec![
            "let draw_1 = [0,",
            " 0,  // or any other generated value",
            " 1",
            "];",
        ]
    );
}

#[test]
fn fully_irrelevant_draws_report_the_together_note() {
    let lines = failing_lines(|tc| {
        let _a: bool = tc.draw(gs::booleans());
        let _b: bool = tc.draw(gs::booleans());
        panic!("always fails");
    });
    assert_eq!(
        lines,
        vec![
            "// The test always failed when commented parts were varied together.",
            "let draw_1 = false;  // or any other generated value",
            "let draw_2 = false;  // or any other generated value",
        ]
    );
}

#[test]
fn together_note_reports_when_varying_everything_sometimes_passes() {
    let lines = failing_lines(|tc| {
        let a: bool = tc.draw(gs::booleans());
        let b: bool = tc.draw(gs::booleans());
        assert!(a && b, "boom");
    });
    assert_eq!(
        lines,
        vec![
            "// The test sometimes passed when commented parts were varied together.",
            "let draw_1 = false;  // or any other generated value",
            "let draw_2 = false;  // or any other generated value",
        ]
    );
}

#[test]
fn tuple_elements_are_annotated_individually() {
    let lines = failing_lines(|tc| {
        let (_a, b): (i32, i32) = tc.draw(hegel::tuples!(gs::integers(), gs::integers()));
        assert!(b < 0, "boom");
    });
    assert_eq!(
        lines,
        vec![
            "let draw_1 = (0,  // or any other generated value",
            " 0",
            ");",
        ]
    );
}

#[test]
fn print_as_value_generators_degrade_to_whole_value_annotations() {
    let lines = failing_lines(|tc| {
        let (_a, b): (i32, i32) =
            tc.draw(hegel::tuples!(gs::integers(), gs::integers()).print_as_value());
        assert!(b < 0, "boom");
    });
    assert_eq!(lines, vec!["let draw_1 = (0, 0);"]);
}

#[test]
fn annotations_survive_database_replay() {
    fn body(tc: hegel::TestCase) {
        let _ignored: i32 = tc.draw(gs::integers());
        let b: i32 = tc.draw(gs::integers());
        assert!(b < 0, "boom");
    }
    let db_dir = tempfile::TempDir::new().unwrap();
    let db_path = db_dir.path().to_str().unwrap().to_string();
    let settings = || {
        Settings::new()
            .test_cases(50)
            .database(Some(db_path.clone()))
            .derandomize(true)
            .report_multiple_failures(false)
    };
    let run_once = || {
        let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let writer = buf.clone();
        let sink: Arc<dyn Fn(&str) + Send + Sync> =
            Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));
        let result = catch_unwind(AssertUnwindSafe(|| {
            hegel::with_output_override(sink, || {
                Hegel::new(body)
                    .settings(settings())
                    .__database_key("annotations_survive_database_replay".to_string())
                    .run();
            });
        }));
        assert!(result.is_err(), "expected the property to fail");
        let mut lines = buf.lock().unwrap().clone();
        if let Some(pos) = lines
            .iter()
            .position(|l| l.starts_with("thread '") && l.contains("panicked at"))
        {
            lines.truncate(pos);
        }
        lines
    };
    let expected = vec![
        "let draw_1 = 0;  // or any other generated value".to_string(),
        "let draw_2 = 0;".to_string(),
    ];
    let first = run_once();
    assert_eq!(first, expected);
    let replayed = run_once();
    assert_eq!(replayed, expected);
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("")
        .to_string()
}

// Run body twice against the same database key: the first run stores a
// failure, then `flip` is set and the second run replays it. Returns the
// second run's panic message.
fn replay_explain_second_run_panic(
    body: fn(hegel::TestCase),
    key: &str,
    flip: &'static AtomicBool,
) -> String {
    let db_dir = tempfile::TempDir::new().unwrap();
    let db_path = db_dir.path().to_str().unwrap().to_string();
    let run = |db: String| {
        catch_unwind(AssertUnwindSafe(move || {
            Hegel::new(body)
                .settings(
                    Settings::new()
                        .test_cases(50)
                        .database(Some(db))
                        .derandomize(true)
                        .report_multiple_failures(false)
                        .verbosity(hegel::Verbosity::Quiet),
                )
                .__database_key(key.to_string())
                .run();
        }))
    };
    run(db_path.clone()).expect_err("the first run must fail and store");
    flip.store(true, Ordering::SeqCst);
    panic_message(run(db_path).expect_err("the second run must error"))
}

// The replayed failure vanishes between the reuse replay and the explain
// phase's verification run: that is flakiness and must be reported as such.
#[test]
fn a_failure_vanishing_before_replay_explain_is_flaky() {
    static RUN2: AtomicBool = AtomicBool::new(false);
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn body(tc: hegel::TestCase) {
        let b: i32 = tc.draw(gs::integers());
        if RUN2.load(Ordering::SeqCst) {
            assert!(CALLS.fetch_add(1, Ordering::SeqCst) != 0, "boom");
        } else {
            assert!(b < 0, "boom");
        }
    }
    let msg = replay_explain_second_run_panic(body, "vanishing_before_replay_explain", &RUN2);
    assert!(msg.contains("Flaky test detected"), "got: {msg:?}");
}

// The test draws a different choice shape during the explain phase's
// verification run than the reuse replay recorded: non-deterministic data
// generation, reported as such.
#[test]
fn nondeterministic_generation_during_replay_explain_is_reported() {
    static RUN2: AtomicBool = AtomicBool::new(false);
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn body(tc: hegel::TestCase) {
        if RUN2.load(Ordering::SeqCst) && CALLS.fetch_add(1, Ordering::SeqCst) >= 1 {
            let _flipped: bool = tc.draw_silent(gs::booleans());
            return;
        }
        let b: i32 = tc.draw(gs::integers());
        assert!(b < 0, "boom");
    }
    let msg = replay_explain_second_run_panic(body, "nondeterministic_replay_explain", &RUN2);
    assert!(msg.contains("non-deterministic"), "got: {msg:?}");
}

#[test]
fn disabling_the_explain_phase_disables_annotations() {
    let lines = failing_lines_with(
        |tc| {
            let _ignored: i32 = tc.draw(gs::integers());
            let b: i32 = tc.draw(gs::integers());
            assert!(b < 0, "boom");
        },
        Settings::new()
            .test_cases(50)
            .database(None)
            .derandomize(true)
            .phases([
                Phase::Explicit,
                Phase::Reuse,
                Phase::Generate,
                Phase::Target,
                Phase::Shrink,
            ]),
    );
    assert_eq!(lines, vec!["let draw_1 = 0;", "let draw_2 = 0;"]);
}

#[test]
fn derived_generator_fields_are_annotated_individually() {
    #[derive(hegel::DefaultGenerator)]
    struct Sonar {
        depth: i64,
        #[allow(dead_code)]
        label: bool,
    }
    let lines = failing_lines(|tc| {
        let sonar: Sonar = tc.draw(gs::default::<Sonar>());
        assert!(sonar.depth < 0, "boom");
    });
    assert_eq!(
        lines,
        vec![
            "let draw_1 = Sonar {",
            "    depth: 0,",
            "    label: false  // or any other generated value",
            "};",
        ]
    );
}

#[test]
fn explain_output_is_deterministic_and_well_formed() {
    fn body(tc: hegel::TestCase) {
        let v: Vec<(i64, Option<bool>)> = tc.draw(
            gs::vecs(hegel::tuples!(gs::integers(), gs::optional(gs::booleans())))
                .min_size(2)
                .max_size(4),
        );
        let s: String = tc.draw(gs::text().max_size(3));
        assert!(v[0].0 < 0 || s.len() > 5, "boom");
    }
    let first = failing_lines(body);
    let second = failing_lines(body);
    assert_eq!(first, second, "derandomized replays must print identically");
    assert!(
        first
            .iter()
            .any(|line| line.contains("// or any other generated value")),
        "this failure has freely variable structure: {first:?}"
    );
    for line in &first {
        if let Some(position) = line.find("//") {
            assert!(
                position == 0 || line[..position].ends_with("  "),
                "a comment is set off by two spaces or starts the line: {line:?}"
            );
            let comment = &line[position..];
            assert!(
                comment == "// or any other generated value" || comment.starts_with("// The test "),
                "comments extend to the end of their line: {line:?}"
            );
        }
    }
}

#[test]
fn annotations_inside_silent_draws_are_dropped() {
    let lines = failing_lines(|tc| {
        let _hidden: i32 = tc.draw_silent(gs::integers());
        let b: i32 = tc.draw(gs::integers());
        assert!(b < 0, "boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 0;"]);
}
