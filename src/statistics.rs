//! Run-level statistics aggregation for the observability prototype.
//!
//! When [`Settings::statistics`](crate::Settings::statistics) (or the
//! `HEGEL_STATISTICS` environment variable) is enabled, events recorded by
//! test bodies through [`TestCase::event`](crate::TestCase::event) and
//! [`TestCase::event_value`](crate::TestCase::event_value) are aggregated
//! across the run and reported when the run finishes, together with a
//! breakdown of test-case outcomes.

use crate::backend::TestCaseResult;
use std::collections::{BTreeMap, BTreeSet};

/// Events recorded by a single test case, buffered on the `TestCase` and
/// merged into the run's [`RunStats`] once the case's outcome is known.
#[derive(Default)]
pub(crate) struct CaseEvents {
    pub(crate) counters: Vec<String>,
    pub(crate) values: Vec<(String, f64)>,
}

#[derive(Default)]
pub(crate) struct RunStats {
    valid: u64,
    invalid: u64,
    overrun: u64,
    interesting: u64,
    counter_totals: BTreeMap<String, u64>,
    counter_cases: BTreeMap<String, u64>,
    values: BTreeMap<String, Vec<f64>>,
}

impl RunStats {
    pub(crate) fn record_case(&mut self, result: &TestCaseResult, events: CaseEvents) {
        match result {
            TestCaseResult::Valid => self.valid += 1,
            TestCaseResult::Invalid => self.invalid += 1,
            TestCaseResult::Overrun => self.overrun += 1,
            TestCaseResult::Interesting(_) => self.interesting += 1,
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for label in &events.counters {
            if seen.insert(label) {
                *self.counter_cases.entry(label.clone()).or_insert(0) += 1;
            }
        }
        for label in events.counters {
            *self.counter_totals.entry(label).or_insert(0) += 1;
        }
        for (label, value) in events.values {
            self.values.entry(label).or_default().push(value);
        }
    }

    pub(crate) fn render(&self, database_key: Option<&str>) -> String {
        let mut out = match database_key {
            Some(key) => format!("Hegel statistics for {key}:\n"),
            None => "Hegel statistics:\n".to_string(),
        };
        let total_cases = self.valid + self.invalid + self.overrun + self.interesting;
        out.push_str(&format!(
            "  - Test cases: {} passing, {} rejected, {} out-of-data, {} failing\n",
            self.valid, self.invalid, self.overrun, self.interesting
        ));
        if !self.counter_totals.is_empty() {
            out.push_str("  - Events:\n");
            for (label, total) in &self.counter_totals {
                let cases = self.counter_cases[label];
                let pct = 100.0 * cases as f64 / total_cases.max(1) as f64;
                out.push_str(&format!(
                    "    - {label}: {total} times in {cases} of {total_cases} test cases ({pct:.1}%)\n"
                ));
            }
        }
        if !self.values.is_empty() {
            out.push_str("  - Observations:\n");
            for (label, values) in &self.values {
                let mut sorted = values.clone();
                sorted.sort_by(f64::total_cmp);
                let n = sorted.len();
                let mean = sorted.iter().sum::<f64>() / n as f64;
                out.push_str(&format!(
                    "    - {label}: n={n} min={} p25={} median={} p75={} p90={} max={} mean={}\n",
                    fmt_value(sorted[0]),
                    fmt_value(percentile(&sorted, 0.25)),
                    fmt_value(percentile(&sorted, 0.5)),
                    fmt_value(percentile(&sorted, 0.75)),
                    fmt_value(percentile(&sorted, 0.9)),
                    fmt_value(sorted[n - 1]),
                    fmt_value(mean),
                ));
            }
        }
        out
    }
}

/// Nearest-rank percentile of an ascending-sorted, non-empty slice.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[index]
}

fn fmt_value(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.3}")
    }
}

#[cfg(test)]
#[path = "../tests/embedded/statistics_tests.rs"]
mod tests;
