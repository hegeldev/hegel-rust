//! Shrink-quality cases distilled from bugs hegel found in real crates (the hegel-zoo), via
//! <https://github.com/hegeldev/shrinking-workbench>. Each case replicates a failing zoo test's
//! draw structure and predicate with no dependency on the upstream crate, and asserts the
//! shortlex ideal: the smallest choice sequence that fails. Where a human would write something
//! else, the case says so.
//!
//! The cases run a plain seeded failing property — no database, no
//! `report_multiple_failures` — and sweep seeds, because what varies between runs is the
//! starting point the shrinker is handed. `common::utils::minimal` keeps generating after the
//! first failure and adopts any smaller failing example it happens upon, which hides exactly what
//! these cases are about.
//!
//! Cases the shrinker does not yet handle are `#[ignore]`d with the mechanism as the reason
//! (`cargo test --test test_shrink_quality zoo -- --ignored` runs them); a listed case that
//! starts passing has its `#[ignore]` removed and becomes a regression test. The distilled
//! cases (`distilled_*`) isolate one mechanism each in two or three integer draws.

mod bytesize_1_sparse_failures;
mod chrono_5_division_roundtrip;
mod chrono_6_product_bound;
mod chrono_delta;
mod control_two_coupled_integers;
mod debian_control_12_budget_after_strings;
mod distilled_lower_and_regenerate;
mod distilled_product_tradeoff;
mod distilled_raise_and_delete;
mod distilled_sparse_multiples;
mod euclid_2_overflowing_difference;
mod euclid_3_degenerate_boxes;
mod format_num_1_gate_then_pick;
mod hcl_rs_2_float_magnitude;
mod hcl_rs_3_shrink_budget;
mod humansize_2_sparse_rounding;
mod jsonparser_9_gate_branch;
mod kurbo_1_segment_endpoints;
mod kurbo_3_underflow_region;
mod ordered_float_1_irrelevant_float;
mod sprintf_13_irrelevant_argument;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

use crate::common::utils::try_measure_failing_run;
use hegel::TestCase;

/// Run the property "`fails(draw(tc))` panics" from consecutive seeds until `runs` of them have
/// found the failure within `test_cases` test cases each, and assert that the shrinker ended at
/// `ideal` every time. Values are compared by their `Debug` output; on failure the message lists
/// every final value with the seeds that ended there.
pub fn assert_shrinks_to<T, D, P>(ideal: &T, runs: usize, test_cases: u64, draw: D, fails: P)
where
    T: Debug,
    D: Fn(&TestCase) -> T + Send + Sync + 'static,
    P: Fn(&T) -> bool + Send + Sync + 'static,
{
    let (draw, fails) = (Arc::new(draw), Arc::new(fails));
    let mut finals: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut found = 0;
    for seed in 0.. {
        assert!(
            seed < 10_000,
            "only {found}/{runs} seeds found the failure within {test_cases} test cases"
        );
        let (draw, fails) = (Arc::clone(&draw), Arc::clone(&fails));
        let Some(stats) = try_measure_failing_run(seed, test_cases, move |tc| {
            let value = draw(tc);
            fails(&value).then(|| format!("{value:?}"))
        }) else {
            continue;
        };
        finals.entry(stats.minimal_repr).or_default().push(seed);
        found += 1;
        if found == runs {
            break;
        }
    }
    let ideal = format!("{ideal:?}");
    let reached = finals.get(&ideal).map_or(0, Vec::len);
    let table: Vec<String> = finals
        .iter()
        .map(|(value, seeds)| format!("  {value}  <- seeds {seeds:?}"))
        .collect();
    assert!(
        reached == runs,
        "shrinker reached {ideal} from {reached}/{runs} seeds; finals by seed:\n{}",
        table.join("\n")
    );
}
