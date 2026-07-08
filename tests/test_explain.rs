use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Phase, Settings};
use std::panic::{AssertUnwindSafe, catch_unwind};
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
fn annotations_inside_silent_draws_are_dropped() {
    let lines = failing_lines(|tc| {
        let _hidden: i32 = tc.draw_silent(gs::integers());
        let b: i32 = tc.draw(gs::integers());
        assert!(b < 0, "boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 0;"]);
}
