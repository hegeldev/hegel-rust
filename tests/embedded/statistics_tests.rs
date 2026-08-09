use super::*;
use crate::backend::Failure;

#[test]
fn test_percentile_nearest_rank() {
    let values = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(percentile(&values, 0.0), 1.0);
    assert_eq!(percentile(&values, 0.5), 3.0);
    assert_eq!(percentile(&values, 1.0), 4.0);
    assert_eq!(percentile(&[7.0], 0.9), 7.0);
}

#[test]
fn test_fmt_value_integers_and_fractions() {
    assert_eq!(fmt_value(3.0), "3");
    assert_eq!(fmt_value(-12.0), "-12");
    assert_eq!(fmt_value(4.32109), "4.321");
    assert_eq!(fmt_value(1e18), "1000000000000000000.000");
}

#[test]
fn test_render_reports_every_outcome_kind() {
    let mut stats = RunStats::default();
    stats.record_case(&TestCaseResult::Valid, CaseEvents::default());
    stats.record_case(&TestCaseResult::Invalid, CaseEvents::default());
    stats.record_case(&TestCaseResult::Overrun, CaseEvents::default());
    stats.record_case(
        &TestCaseResult::Interesting(Failure {
            origin: "somewhere".to_string(),
        }),
        CaseEvents::default(),
    );
    let report = stats.render(Some("my_test"));
    assert!(report.contains("Hegel statistics for my_test:"));
    assert!(report.contains("Test cases: 1 passing, 1 rejected, 1 out-of-data, 1 failing"));
    assert!(!report.contains("Events:"));
    assert!(!report.contains("Observations:"));
}

#[test]
fn test_render_counts_occurrences_and_cases_separately() {
    let mut stats = RunStats::default();
    stats.record_case(
        &TestCaseResult::Valid,
        CaseEvents {
            counters: vec!["hit".to_string(), "hit".to_string()],
            values: vec![],
        },
    );
    stats.record_case(&TestCaseResult::Valid, CaseEvents::default());
    let report = stats.render(None);
    assert!(report.contains("hit: 2 times in 1 of 2 test cases (50.0%)"));
}

#[test]
fn test_render_value_distribution() {
    let mut stats = RunStats::default();
    for value in [4.0, 1.0, 3.0, 2.0] {
        stats.record_case(
            &TestCaseResult::Valid,
            CaseEvents {
                counters: vec![],
                values: vec![("size".to_string(), value)],
            },
        );
    }
    let report = stats.render(None);
    assert!(
        report.contains("size: n=4 min=1 p25=2 median=3 p75=3 p90=4 max=4 mean=2.500"),
        "unexpected report:\n{report}"
    );
}
