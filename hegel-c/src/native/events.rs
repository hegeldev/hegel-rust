//! Per-run aggregation of `tc.event()` / `tc.event_value()` observations,
//! reported at the end of the run when the `show_statistics` setting is on.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Run-level event statistics, folded in one test case at a time.
///
/// Only generation-phase cases with status Valid or Interesting are
/// recorded: replays during shrinking would re-observe the same shrinking
/// target over and over and swamp the distribution the user asked about.
#[derive(Default)]
pub(crate) struct RunStatistics {
    /// Test cases folded in, the denominator for event percentages.
    cases: u64,
    /// Per label: the number of recorded cases in which the label was
    /// observed at least once.
    counts: BTreeMap<String, u64>,
    /// Per label: every numeric observation from recorded cases.
    values: BTreeMap<String, Vec<f64>>,
}

impl RunStatistics {
    /// Fold in one test case's observations. Bare events are deduplicated
    /// per case (the report says "in what fraction of cases did this
    /// happen"); numeric observations all count.
    pub(crate) fn record_case(&mut self, events: &[(String, Option<f64>)]) {
        self.cases += 1;
        let mut seen: Vec<&str> = Vec::new();
        for (label, value) in events {
            match value {
                None => {
                    if !seen.contains(&label.as_str()) {
                        seen.push(label);
                        *self.counts.entry(label.clone()).or_insert(0) += 1;
                    }
                }
                Some(v) => self.values.entry(label.clone()).or_default().push(*v),
            }
        }
    }

    /// Render the end-of-run statistics block.
    pub(crate) fn render(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.counts.is_empty() && self.values.is_empty() {
            lines.push(format!(
                "Statistics (over {} test cases): no events were recorded; \
                 record them with tc.event(..) or tc.event_value(..)",
                self.cases
            ));
            return lines;
        }
        lines.push(format!("Statistics (over {} test cases):", self.cases));
        for (label, count) in &self.counts {
            let percent = 100.0 * *count as f64 / self.cases.max(1) as f64;
            lines.push(format!("  * {label}: {percent:.1}% of test cases"));
        }
        for (label, values) in &self.values {
            let mut sorted = values.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let count = sorted.len();
            let min = sorted[0];
            let max = sorted[count - 1];
            let median = if count % 2 == 1 {
                sorted[count / 2]
            } else {
                (sorted[count / 2 - 1] + sorted[count / 2]) / 2.0
            };
            let mean = sorted.iter().sum::<f64>() / count as f64;
            let p90 = sorted[nearest_rank_index(count, 0.9)];
            lines.push(format!(
                "  * {label}: count {count}, min {min}, median {median}, \
                 mean {mean:.2}, p90 {p90}, max {max}"
            ));
        }
        lines
    }
}

/// Nearest-rank index of quantile `q` in a sorted list of `count`
/// observations: the smallest index whose rank covers `q` of the list.
/// Distribution tails are where generation problems hide (a collapse can
/// move a high percentile long before it moves the maximum), so the report
/// includes p90 alongside the extremes.
fn nearest_rank_index(count: usize, q: f64) -> usize {
    let rank = libm::ceil(count as f64 * q) as usize;
    rank.max(1) - 1
}

#[cfg(test)]
#[path = "../../tests/embedded/native/events_tests.rs"]
mod tests;
