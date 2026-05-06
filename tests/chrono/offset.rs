use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use chrono::FixedOffset;
use hegel::extras::chrono as chrono_gs;
use hegel::generators::{self as gs, Generator};

// ---------------------------------------------------------------------------
// FixedOffset
// ---------------------------------------------------------------------------

#[test]
fn test_fixed_offsets_default() {
    assert_all_examples(chrono_gs::fixed_offsets(), |o| {
        let s = o.local_minus_utc();
        (-86_399..=86_399).contains(&s)
    });
}

#[test]
fn test_fixed_offsets_min_value() {
    let min = FixedOffset::east_opt(-3600).unwrap();
    assert_all_examples(chrono_gs::fixed_offsets().min_value(min), move |o| {
        o.local_minus_utc() >= -3600
    });
}

#[test]
fn test_fixed_offsets_max_value() {
    let max = FixedOffset::east_opt(3600).unwrap();
    assert_all_examples(chrono_gs::fixed_offsets().max_value(max), move |o| {
        o.local_minus_utc() <= 3600
    });
}

#[test]
fn test_fixed_offsets_in_vec() {
    let max = FixedOffset::east_opt(3600).unwrap();
    assert_all_examples(
        gs::vecs(chrono_gs::fixed_offsets().max_value(max)).max_size(5),
        move |v| v.iter().all(|o| o.local_minus_utc() <= 3600),
    );
}

#[test]
fn test_fixed_offset_default_generator() {
    check_can_generate_examples(gs::default::<FixedOffset>());
}

#[hegel::test]
fn test_fixed_offsets_property(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<i32>().min_value(-86_399).max_value(86_399));
    let hi = tc.draw(gs::integers::<i32>().min_value(lo).max_value(86_399));
    let min = FixedOffset::east_opt(lo).unwrap();
    let max = FixedOffset::east_opt(hi).unwrap();
    let v = tc.draw(chrono_gs::fixed_offsets().min_value(min).max_value(max));
    assert!(v.local_minus_utc() >= lo);
    assert!(v.local_minus_utc() <= hi);
}

#[test]
#[should_panic(expected = "max_value < min_value")]
fn test_fixed_offsets_min_greater_than_max() {
    let g = chrono_gs::fixed_offsets()
        .min_value(FixedOffset::east_opt(3600).unwrap())
        .max_value(FixedOffset::east_opt(-3600).unwrap());
    g.as_basic();
}
