use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use hegel::extras::jiff as jiff_gs;
use hegel::generators::{self as gs, Generator};
use jiff::tz::{AmbiguousOffset, Offset, TimeZone};
use jiff::{Timestamp, Zoned};

// ---------------------------------------------------------------------------
// tz::Offset
// ---------------------------------------------------------------------------

#[test]
fn test_jiff_offsets_default() {
    assert_all_examples(jiff_gs::offsets(), |o| {
        (-93_599..=93_599).contains(&o.seconds())
    });
}

#[test]
fn test_jiff_offsets_min_value() {
    let min = Offset::from_seconds(-3600).unwrap();
    assert_all_examples(jiff_gs::offsets().min_value(min), move |o| o >= &min);
}

#[test]
fn test_jiff_offsets_max_value() {
    let max = Offset::from_seconds(3600).unwrap();
    assert_all_examples(jiff_gs::offsets().max_value(max), move |o| o <= &max);
}

#[test]
fn test_jiff_offsets_in_vec() {
    let max = Offset::from_seconds(3600).unwrap();
    assert_all_examples(
        gs::vecs(jiff_gs::offsets().max_value(max)).max_size(5),
        move |v| v.iter().all(|o| o.seconds() <= 3600),
    );
}

#[test]
fn test_jiff_offset_default_generator() {
    check_can_generate_examples(gs::default::<Offset>());
}

#[hegel::test]
fn test_jiff_offsets_property(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<i32>().min_value(-93_599).max_value(93_599));
    let hi = tc.draw(gs::integers::<i32>().min_value(lo).max_value(93_599));
    let min = Offset::from_seconds(lo).unwrap();
    let max = Offset::from_seconds(hi).unwrap();
    let v = tc.draw(jiff_gs::offsets().min_value(min).max_value(max));
    assert!(v.seconds() >= lo && v.seconds() <= hi);
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_jiff_offsets_min_greater_than_max() {
    let g = jiff_gs::offsets()
        .min_value(Offset::from_seconds(10).unwrap())
        .max_value(Offset::from_seconds(5).unwrap());
    g.as_basic();
}

// ---------------------------------------------------------------------------
// tz::TimeZone
// ---------------------------------------------------------------------------

#[test]
fn test_jiff_timezone_default_generator() {
    check_can_generate_examples(gs::default::<TimeZone>());
}

#[test]
fn test_jiff_timezones_in_vec() {
    // every generated TimeZone should be queryable; it's hard to assert much
    // beyond non-panic since the values are heterogeneous.
    assert_all_examples(gs::vecs(gs::default::<TimeZone>()).max_size(5), |v| {
        v.iter()
            .all(|tz: &TimeZone| !tz.iana_name().unwrap_or("").contains('\0'))
    });
}

// ---------------------------------------------------------------------------
// Zoned
// ---------------------------------------------------------------------------

#[test]
fn test_jiff_zoneds_default() {
    check_can_generate_examples(jiff_gs::zoneds());
}

#[test]
fn test_jiff_zoneds_in_vec() {
    assert_all_examples(gs::vecs(jiff_gs::zoneds()).max_size(3), |v| {
        v.iter().all(|z| z.year() >= -9999 && z.year() <= 9999)
    });
}

#[test]
fn test_jiff_zoned_default_generator() {
    check_can_generate_examples(gs::default::<Zoned>());
}

#[test]
fn test_jiff_zoneds_with_custom_timezones() {
    // Replace the timezone generator and verify every produced Zoned uses UTC.
    assert_all_examples(jiff_gs::zoneds().timezones(gs::just(TimeZone::UTC)), |z| {
        z.time_zone() == &TimeZone::UTC
    });
}

#[test]
fn test_jiff_zoneds_with_custom_timestamps() {
    // Replace the timestamp generator with a fixed value.
    let fixed = Timestamp::from_second(0).unwrap();
    assert_all_examples(jiff_gs::zoneds().timestamps(gs::just(fixed)), move |z| {
        z.timestamp() == fixed
    });
}

// ---------------------------------------------------------------------------
// tz::AmbiguousOffset
// ---------------------------------------------------------------------------

#[test]
fn test_jiff_ambiguous_offset_default_generator() {
    check_can_generate_examples(gs::default::<AmbiguousOffset>());
}

#[test]
fn test_jiff_ambiguous_offsets_in_vec() {
    assert_all_examples(
        gs::vecs(gs::default::<AmbiguousOffset>()).max_size(5),
        |v| {
            v.iter().all(|o| {
                matches!(
                    o,
                    AmbiguousOffset::Unambiguous { .. }
                        | AmbiguousOffset::Gap { .. }
                        | AmbiguousOffset::Fold { .. }
                )
            })
        },
    );
}
