use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::extras::jiff as jiff_gs;
use hegel::generators::{self as gs, Generator};
use jiff::{SignedDuration, Span, Timestamp};

#[test]
fn test_jiff_timestamps_default() {
    check_can_generate_examples(jiff_gs::timestamps());
}

#[test]
fn test_jiff_timestamps_min_value() {
    let min = Timestamp::UNIX_EPOCH;
    assert_all_examples(jiff_gs::timestamps().min_value(min), move |t| *t >= min);
}

#[test]
fn test_jiff_timestamps_max_value() {
    let max = Timestamp::UNIX_EPOCH;
    assert_all_examples(jiff_gs::timestamps().max_value(max), move |t| *t <= max);
}

#[test]
fn test_jiff_timestamps_in_vec() {
    let max = Timestamp::UNIX_EPOCH;
    assert_all_examples(
        gs::vecs(jiff_gs::timestamps().max_value(max)).max_size(5),
        move |v| v.iter().all(|t| *t <= max),
    );
}

#[test]
fn test_jiff_timestamp_default_generator() {
    check_can_generate_examples(gs::default::<Timestamp>());
}

#[hegel::test]
fn test_jiff_timestamps_property(tc: hegel::TestCase) {
    let lo_secs = tc.draw(
        gs::integers::<i64>()
            .min_value(Timestamp::MIN.as_second())
            .max_value(Timestamp::MAX.as_second()),
    );
    let hi_secs = tc.draw(
        gs::integers::<i64>()
            .min_value(lo_secs)
            .max_value(Timestamp::MAX.as_second()),
    );
    let lo = Timestamp::from_second(lo_secs).unwrap();
    let hi = Timestamp::from_second(hi_secs).unwrap();
    let v = tc.draw(jiff_gs::timestamps().min_value(lo).max_value(hi));
    assert!(v >= lo && v <= hi);
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_jiff_timestamps_min_greater_than_max() {
    let g = jiff_gs::timestamps()
        .min_value(Timestamp::from_second(10).unwrap())
        .max_value(Timestamp::from_second(5).unwrap());
    g.as_basic();
}

#[test]
fn test_jiff_spans_default() {
    check_can_generate_examples(jiff_gs::spans());
}

#[test]
fn test_jiff_spans_min_nanoseconds() {
    assert_all_examples(jiff_gs::spans().min_nanoseconds(0), |s| {
        s.get_nanoseconds() >= 0
    });
}

#[test]
fn test_jiff_spans_max_nanoseconds() {
    assert_all_examples(jiff_gs::spans().max_nanoseconds(0), |s| {
        s.get_nanoseconds() <= 0
    });
}

#[test]
fn test_jiff_spans_in_vec() {
    assert_all_examples(
        gs::vecs(jiff_gs::spans().min_nanoseconds(0).max_nanoseconds(1_000)).max_size(5),
        |v| v.iter().all(|s| (0..=1_000).contains(&s.get_nanoseconds())),
    );
}

#[test]
fn test_jiff_span_default_generator() {
    check_can_generate_examples(gs::default::<Span>());
}

#[hegel::test]
fn test_jiff_spans_property(tc: hegel::TestCase) {
    let lo = tc.draw(
        gs::integers::<i64>()
            .min_value(-i64::MAX)
            .max_value(i64::MAX),
    );
    let hi = tc.draw(gs::integers::<i64>().min_value(lo).max_value(i64::MAX));
    let s = tc.draw(jiff_gs::spans().min_nanoseconds(lo).max_nanoseconds(hi));
    let n = s.get_nanoseconds();
    assert!(n >= lo && n <= hi);
}

#[test]
#[should_panic(expected = "max_nanoseconds < min_nanoseconds")]
fn test_jiff_spans_min_greater_than_max() {
    let g = jiff_gs::spans().min_nanoseconds(10).max_nanoseconds(5);
    g.as_basic();
}

#[test]
#[should_panic(expected = "min_nanoseconds must be >= -i64::MAX")]
fn test_jiff_spans_min_below_span_limit() {
    let g = jiff_gs::spans().min_nanoseconds(i64::MIN);
    g.as_basic();
}

#[test]
fn test_jiff_signed_durations_default() {
    check_can_generate_examples(jiff_gs::signed_durations());
}

#[test]
fn test_jiff_signed_durations_min_value() {
    assert_all_examples(
        jiff_gs::signed_durations().min_value(SignedDuration::ZERO),
        |d| d.as_nanos() >= 0,
    );
}

#[test]
fn test_jiff_signed_durations_max_value() {
    assert_all_examples(
        jiff_gs::signed_durations().max_value(SignedDuration::ZERO),
        |d| d.as_nanos() <= 0,
    );
}

#[test]
fn test_jiff_signed_durations_in_vec() {
    let max = SignedDuration::from_nanos(1_000);
    assert_all_examples(
        gs::vecs(
            jiff_gs::signed_durations()
                .min_value(SignedDuration::ZERO)
                .max_value(max),
        )
        .max_size(5),
        move |v| {
            v.iter()
                .all(|d| d.as_nanos() >= 0 && d.as_nanos() <= max.as_nanos())
        },
    );
}

#[test]
fn test_jiff_signed_duration_default_generator() {
    check_can_generate_examples(gs::default::<SignedDuration>());
}

#[hegel::test]
fn test_jiff_signed_durations_property(tc: hegel::TestCase) {
    let lo_n = tc.draw(gs::integers::<i64>());
    let hi_n = tc.draw(gs::integers::<i64>().min_value(lo_n));
    let lo = SignedDuration::from_nanos(lo_n);
    let hi = SignedDuration::from_nanos(hi_n);
    let d = tc.draw(jiff_gs::signed_durations().min_value(lo).max_value(hi));
    assert!(d >= lo && d <= hi);
}

#[test]
fn test_jiff_signed_durations_full_range_bounds() {
    check_can_generate_examples(
        jiff_gs::signed_durations()
            .min_value(SignedDuration::MIN)
            .max_value(SignedDuration::MAX),
    );
}

#[test]
fn test_jiff_signed_durations_beyond_i64_nanos() {
    let one_kyear = SignedDuration::from_secs(1_000 * 365 * 86_400);
    let two_kyears = SignedDuration::from_secs(2_000 * 365 * 86_400);
    assert_all_examples(
        jiff_gs::signed_durations()
            .min_value(one_kyear)
            .max_value(two_kyears),
        move |d| *d >= one_kyear && *d <= two_kyears,
    );
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_jiff_signed_durations_min_greater_than_max() {
    let g = jiff_gs::signed_durations()
        .min_value(SignedDuration::from_nanos(10))
        .max_value(SignedDuration::from_nanos(5));
    g.as_basic();
}
