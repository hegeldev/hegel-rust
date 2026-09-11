//! Shrink benchmark mirroring the jiff negative-fractional-duration bug from
//! the OSS bug-finding dataset: the failing property draws a rounding unit
//! plus a full-range nanosecond count, and fails exactly when the resulting
//! span is negative with only fractional (sub-second) magnitude.

use crate::common::utils::measure_failing_run;
use hegel::generators::{self as gs, Generator};
use jiff::civil::DateTime;
use jiff::{Span, SpanRelativeTo, SpanRound, Unit};

#[test]
fn shrink_budget_jiff_negative_fractional_span() {
    let stats = measure_failing_run(100, 100, |tc| {
        let units: Vec<Unit> = vec![
            Unit::Year,
            Unit::Month,
            Unit::Week,
            Unit::Day,
            Unit::Hour,
            Unit::Minute,
            Unit::Second,
            Unit::Millisecond,
            Unit::Microsecond,
            Unit::Nanosecond,
        ];
        let unit = tc.draw(gs::sampled_from(units).print_as_debug());
        let nanos = tc.draw(
            gs::integers::<i64>()
                .min_value(-i64::MAX)
                .max_value(i64::MAX),
        );
        let relative = SpanRelativeTo::from(DateTime::constant(0, 1, 1, 0, 0, 0, 0));
        let round = SpanRound::new().largest(unit).relative(relative);
        let span = Span::new().nanoseconds(nanos).round(round).unwrap();
        (nanos < 0 && nanos > -1_000_000_000)
            .then(|| format!("unit={unit:?} nanos={nanos} span={span}"))
    });
    assert_eq!(
        stats.minimal_repr,
        "unit=Year nanos=-1 span=-PT0.000000001S"
    );
    assert!(
        stats.post_discovery_calls() < 400,
        "took {} post-discovery test-body calls",
        stats.post_discovery_calls()
    );
}
