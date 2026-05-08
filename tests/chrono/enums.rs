use crate::common::utils::{assert_all_examples, check_can_generate_examples};
use chrono::{Month, Weekday};
use hegel::extras::chrono as chrono_gs;
use hegel::generators as gs;

// ---------------------------------------------------------------------------
// Weekday
// ---------------------------------------------------------------------------

#[test]
fn test_weekdays_default() {
    check_can_generate_examples(gs::default::<Weekday>());
}

#[test]
fn test_weekdays_in_vec() {
    assert_all_examples(gs::vecs(gs::default::<Weekday>()).max_size(7), |v| {
        v.iter()
            .all(|w| (0..=6).contains(&w.num_days_from_monday()))
    });
}

// ---------------------------------------------------------------------------
// Month
// ---------------------------------------------------------------------------

#[test]
fn test_months_default() {
    check_can_generate_examples(gs::default::<Month>());
}

#[test]
fn test_months_in_vec() {
    assert_all_examples(gs::vecs(gs::default::<Month>()).max_size(7), |v| {
        v.iter().all(|m| (1..=12).contains(&m.number_from_month()))
    });
}

// ---------------------------------------------------------------------------
// WeekdaySet
// ---------------------------------------------------------------------------

#[test]
fn test_weekday_sets_default() {
    check_can_generate_examples(chrono_gs::weekday_sets());
}

#[test]
fn test_weekday_sets_in_vec() {
    assert_all_examples(gs::vecs(chrono_gs::weekday_sets()).max_size(5), |v| {
        v.iter().all(|s| s.len() <= 7)
    });
}

#[test]
fn test_weekday_set_default_generator() {
    check_can_generate_examples(gs::default::<chrono::WeekdaySet>());
}
