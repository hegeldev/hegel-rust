//! End-of-run event statistics: `tc.event(..)` / `tc.event_value(..)`
//! observations reported under `Settings::show_statistics`, captured
//! through `hegel::with_output_override`.

use hegel::generators as gs;
use hegel::{Hegel, Settings, TestCase};
use std::sync::{Arc, Mutex};

fn capture_run(settings: Settings, body: impl FnMut(TestCase) + 'static) -> Vec<String> {
    let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = Arc::clone(&buf);
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));
    hegel::with_output_override(sink, || {
        Hegel::new(body).settings(settings).run();
    });
    buf.lock().unwrap().clone()
}

fn settings() -> Settings {
    Settings::new()
        .test_cases(50)
        .database(None)
        .derandomize(true)
}

#[test]
fn test_statistics_report_events_and_value_distributions() {
    let lines = capture_run(settings().show_statistics(true), |tc: TestCase| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(9));
        if n == 0 {
            tc.event("zero");
        }
        tc.event_value("n", n as f64);
    });
    assert!(lines.iter().any(|l| l.starts_with("Statistics (over ")));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("* zero: ") && l.contains("% of test cases"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("* n: count ") && l.contains("median"))
    );
}

#[test]
fn test_statistics_are_not_reported_by_default() {
    let lines = capture_run(settings(), |tc: TestCase| {
        tc.draw(gs::booleans());
        tc.event("always");
    });
    assert!(!lines.iter().any(|l| l.contains("Statistics")));
}

#[test]
fn test_events_from_rejected_test_cases_do_not_count() {
    let lines = capture_run(settings().show_statistics(true), |tc: TestCase| {
        if tc.draw(gs::booleans()) {
            tc.event("rejected");
            tc.assume(false);
        }
        tc.event("kept");
    });
    assert!(
        lines
            .iter()
            .any(|l| l.contains("* kept: 100.0% of test cases"))
    );
    assert!(!lines.iter().any(|l| l.contains("* rejected:")));
}

#[hegel::test]
#[should_panic(expected = "finite value")]
fn test_event_value_rejects_non_finite_values(tc: TestCase) {
    tc.event_value("bad", f64::NAN);
}

#[test]
fn test_statistics_without_events_point_at_the_recording_api() {
    let lines = capture_run(settings().show_statistics(true), |tc: TestCase| {
        tc.draw(gs::booleans());
    });
    assert!(lines.iter().any(|l| l.contains("no events were recorded")));
}

#[test]
fn test_statistics_report_the_highest_target_score_per_label() {
    let lines = capture_run(settings().show_statistics(true), |tc: TestCase| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(1).max_value(9));
        tc.target_labelled(n as f64, "n");
        tc.target_labelled(-(n as f64), "minus n");
    });
    let position = lines
        .iter()
        .position(|l| l == "Highest target scores:")
        .unwrap();
    assert!(
        lines[..position]
            .iter()
            .any(|l| l.starts_with("Statistics (over "))
    );
    assert_eq!(lines[position + 1].trim_start(), "-1  (label=\"minus n\")");
    assert_eq!(lines[position + 2].trim_start(), "9  (label=\"n\")");
}

#[test]
fn test_statistics_report_a_single_target_label_on_one_line() {
    let lines = capture_run(settings().show_statistics(true), |tc: TestCase| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(9));
        tc.target(n as f64);
    });
    assert!(lines.contains(&"Highest target score: 9  (label=\"\")".to_string()));
}

#[test]
fn test_target_scores_are_not_reported_without_statistics() {
    let lines = capture_run(settings(), |tc: TestCase| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(9));
        tc.target(n as f64);
    });
    assert!(!lines.iter().any(|l| l.contains("Highest target score")));
}
