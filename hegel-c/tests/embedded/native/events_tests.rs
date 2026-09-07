use super::*;
use alloc::string::ToString;
use alloc::vec;

fn event(label: &str) -> (String, Option<f64>) {
    (label.to_string(), None)
}

fn value(label: &str, v: f64) -> (String, Option<f64>) {
    (label.to_string(), Some(v))
}

#[test]
fn no_events_renders_a_pointer_at_the_recording_api() {
    let mut stats = RunStatistics::default();
    stats.record_case(&[]);
    stats.record_case(&[]);
    let lines = stats.render();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("over 2 test cases"));
    assert!(lines[0].contains("no events were recorded"));
}

#[test]
fn bare_events_report_the_fraction_of_cases_deduplicated_within_a_case() {
    let mut stats = RunStatistics::default();
    stats.record_case(&[event("hit"), event("hit"), event("other")]);
    stats.record_case(&[event("hit")]);
    stats.record_case(&[]);
    stats.record_case(&[]);
    let lines = stats.render();
    assert_eq!(lines[0], "Statistics (over 4 test cases):");
    assert_eq!(lines[1], "  * hit: 50.0% of test cases");
    assert_eq!(lines[2], "  * other: 25.0% of test cases");
}

#[test]
fn numeric_observations_report_a_distribution_summary() {
    let mut stats = RunStatistics::default();
    stats.record_case(&[value("size", 1.0), value("size", 3.0)]);
    stats.record_case(&[value("size", 2.0), value("size", 10.0)]);
    let lines = stats.render();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[1],
        "  * size: count 4, min 1, median 2.5, mean 4.00, p90 10, max 10"
    );
}

#[test]
fn odd_count_median_is_the_middle_observation() {
    let mut stats = RunStatistics::default();
    stats.record_case(&[value("n", 5.0), value("n", 1.0), value("n", 2.0)]);
    let lines = stats.render();
    assert_eq!(
        lines[1],
        "  * n: count 3, min 1, median 2, mean 2.67, p90 5, max 5"
    );
}

#[test]
fn bare_and_numeric_events_render_in_label_order() {
    let mut stats = RunStatistics::default();
    stats.record_case(&[value("a-value", 7.0), event("z-event"), event("a-event")]);
    let lines = stats.render();
    assert_eq!(
        lines,
        vec![
            "Statistics (over 1 test cases):".to_string(),
            "  * a-event: 100.0% of test cases".to_string(),
            "  * z-event: 100.0% of test cases".to_string(),
            "  * a-value: count 1, min 7, median 7, mean 7.00, p90 7, max 7".to_string(),
        ]
    );
}

#[test]
fn p90_is_the_nearest_rank_below_an_outlying_maximum() {
    let mut stats = RunStatistics::default();
    let observations: Vec<(String, Option<f64>)> = (1..=9)
        .map(|n| value("n", n as f64))
        .chain([value("n", 100.0)])
        .collect();
    stats.record_case(&observations);
    let lines = stats.render();
    assert_eq!(
        lines[1],
        "  * n: count 10, min 1, median 5.5, mean 14.50, p90 9, max 100"
    );
}
