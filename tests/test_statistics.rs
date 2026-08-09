mod common;

use common::exec::self_test;
use common::utils::{assert_matches_regex, capture_hegel_output};
use hegel::generators as gs;
use hegel::{Hegel, Settings};

fn stats_settings() -> Settings {
    Settings::new()
        .test_cases(100)
        .database(None)
        .derandomize(true)
        .statistics(true)
}

#[test]
fn test_statistics_report_counts_test_cases() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: hegel::TestCase| {
            tc.draw(gs::integers::<i32>());
        })
        .settings(stats_settings())
        .run();
    });
    result.unwrap();
    let output = lines.join("\n");
    assert_matches_regex(&output, "Hegel statistics:");
    assert_matches_regex(&output, "Test cases: 100 passing, 0 rejected");
}

#[test]
fn test_statistics_report_includes_rejected_cases() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: hegel::TestCase| {
            let n = tc.draw(gs::integers::<i32>().min_value(0).max_value(9999));
            tc.assume(n % 10 != 0);
        })
        .settings(stats_settings())
        .run();
    });
    result.unwrap();
    let output = lines.join("\n");
    assert_matches_regex(&output, r"Test cases: 100 passing, [1-9]\d* rejected");
}

#[test]
fn test_statistics_counter_events_are_aggregated() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: hegel::TestCase| {
            let n = tc.draw(gs::integers::<i32>().min_value(0).max_value(9999));
            if n >= 5000 {
                tc.event("n is large");
                tc.event("n is large");
            }
        })
        .settings(stats_settings())
        .run();
    });
    result.unwrap();
    let output = lines.join("\n");
    assert_matches_regex(
        &output,
        r"n is large: \d+ times in \d+ of 100 test cases \(\d+\.\d%\)",
    );
}

#[test]
fn test_statistics_value_events_report_a_distribution() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: hegel::TestCase| {
            let n = tc.draw(gs::integers::<i32>().min_value(1).max_value(100));
            tc.event_value("n", n as f64);
        })
        .settings(stats_settings())
        .run();
    });
    result.unwrap();
    let output = lines.join("\n");
    assert_matches_regex(
        &output,
        r"n: n=100 min=\d+ p25=\S+ median=\S+ p75=\S+ p90=\S+ max=\d+ mean=\S+",
    );
}

#[test]
fn test_statistics_are_off_by_default() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: hegel::TestCase| {
            tc.draw(gs::integers::<i32>());
            tc.event("something");
        })
        .settings(Settings::new().test_cases(10).database(None))
        .run();
    });
    result.unwrap();
    let output = lines.join("\n");
    assert!(
        !output.contains("Hegel statistics"),
        "statistics must be opt-in:\n{output}"
    );
}

#[test]
fn test_statistics_are_printed_for_failing_runs_without_final_replay_events() {
    let (lines, result) = capture_hegel_output(|| {
        Hegel::new(|tc: hegel::TestCase| {
            let n = tc.draw(gs::integers::<i32>().min_value(0).max_value(1000));
            tc.event("case ran");
            assert!(n < 500);
        })
        .settings(stats_settings())
        .run();
    });
    result.unwrap_err();
    let output = lines.join("\n");
    assert_matches_regex(&output, r"[1-9]\d* failing");
    let (times, cases) = {
        let re = regex::Regex::new(r"case ran: (\d+) times in (\d+) of (\d+) test cases").unwrap();
        let caps = re.captures(&output).unwrap();
        (caps[1].to_string(), caps[2].to_string())
    };
    assert_eq!(times, cases, "each case records the event exactly once");
}

/// Fixture for `test_hegel_statistics_env_enables_statistics`, run via
/// self-exec with `HEGEL_STATISTICS=1`: statistics must be reported even
/// though the settings never enable them.
#[test]
#[ignore = "fixture: run via exec::self_test"]
fn env_statistics_fixture() {
    Hegel::new(|tc: hegel::TestCase| {
        tc.draw(gs::integers::<i32>());
        tc.event("fixture event");
    })
    .settings(Settings::new().test_cases(5).database(None))
    .run();
}

#[test]
fn test_hegel_statistics_env_enables_statistics() {
    let output = self_test("env_statistics_fixture")
        .env("HEGEL_STATISTICS", "1")
        .run();
    assert_matches_regex(&output.stderr, "Hegel statistics:");
    assert_matches_regex(
        &output.stderr,
        "fixture event: 5 times in 5 of 5 test cases",
    );
}
