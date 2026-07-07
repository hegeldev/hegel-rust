use std::net::{Ipv4Addr, Ipv6Addr};

use super::*;
use crate::native::core::NativeTestCase;
use crate::native::rng::EngineRng;

fn fresh_ntc(seed: u64) -> NativeTestCase {
    NativeTestCase::new_random(EngineRng::seeded(seed))
}

fn date(year: i32, month: u8, day: u8) -> Date {
    Date { year, month, day }
}

fn time(hour: u8, minute: u8, second: u8, microsecond: u32) -> Time {
    Time {
        hour,
        minute,
        second,
        microsecond,
    }
}

const FULL_MIN_DATE: Date = Date {
    year: 1,
    month: 1,
    day: 1,
};
const FULL_MAX_DATE: Date = Date {
    year: 9999,
    month: 12,
    day: 31,
};
const MIDNIGHT: Time = Time {
    hour: 0,
    minute: 0,
    second: 0,
    microsecond: 0,
};
const LAST_MICROSECOND: Time = Time {
    hour: 23,
    minute: 59,
    second: 59,
    microsecond: 999_999,
};

#[test]
fn generate_date_produces_valid_dates() {
    for seed in 0..50 {
        let mut ntc = fresh_ntc(seed);
        let d = generate_date(&mut ntc, FULL_MIN_DATE, FULL_MAX_DATE).unwrap();
        assert!((1..=9999).contains(&d.year), "year out of range: {d:?}");
        assert!((1..=12).contains(&d.month), "month out of range: {d:?}");
        assert!((1..=31).contains(&d.day), "day out of range: {d:?}");
    }
}

#[test]
fn generate_date_day_respects_month_length() {
    let mut seen_feb = false;
    for seed in 0..1000 {
        let mut ntc = fresh_ntc(seed);
        let d = generate_date(&mut ntc, FULL_MIN_DATE, FULL_MAX_DATE).unwrap();
        if d.month == 2 {
            seen_feb = true;
            let is_leap = d.year % 4 == 0 && (d.year % 100 != 0 || d.year % 400 == 0);
            let max_day = if is_leap { 29 } else { 28 };
            assert!(d.day <= max_day, "Feb day {} in year {}", d.day, d.year);
        }
        if matches!(d.month, 4 | 6 | 9 | 11) {
            assert!(d.day <= 30, "31st in a 30-day month: {d:?}");
        }
    }
    assert!(seen_feb, "no February dates drawn across 1000 seeds");
}

#[test]
fn generate_time_covers_zero_and_nonzero_microseconds() {
    let mut seen_microsecond_zero = false;
    let mut seen_microsecond_nonzero = false;
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let t = generate_time(&mut ntc, MIDNIGHT, LAST_MICROSECOND).unwrap();
        assert!(t.hour <= 23 && t.minute <= 59 && t.second <= 59, "{t:?}");
        assert!(t.microsecond <= 999_999, "{t:?}");
        if t.microsecond == 0 {
            seen_microsecond_zero = true;
        } else {
            seen_microsecond_nonzero = true;
        }
    }
    assert!(
        seen_microsecond_zero,
        "no zero-microsecond times across 200 seeds"
    );
    assert!(
        seen_microsecond_nonzero,
        "no nonzero-microsecond times across 200 seeds"
    );
}

#[test]
fn generate_datetime_combines_valid_parts() {
    for seed in 0..50 {
        let mut ntc = fresh_ntc(seed);
        let dt = generate_datetime(
            &mut ntc,
            DateTime {
                date: FULL_MIN_DATE,
                time: MIDNIGHT,
            },
            DateTime {
                date: FULL_MAX_DATE,
                time: LAST_MICROSECOND,
            },
        )
        .unwrap();
        assert!((1..=9999).contains(&dt.date.year));
        assert!((1..=12).contains(&dt.date.month));
        assert!(dt.time.hour <= 23);
    }
}

#[test]
fn generate_uuid_respects_version() {
    for version in 1u8..=5 {
        for seed in 0..30 {
            let mut ntc = fresh_ntc(seed);
            let bytes = generate_uuid(&mut ntc, Some(version)).unwrap();
            assert_eq!(bytes[6] >> 4, version, "version nibble mismatch");
            assert!(
                matches!(bytes[8] >> 4, 0x8..=0xb),
                "variant nibble {:x} not in 8..=b",
                bytes[8] >> 4
            );
        }
    }
}

#[test]
fn generate_uuid_default_can_produce_non_rfc_versions() {
    let mut saw_non_rfc = false;
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let bytes = generate_uuid(&mut ntc, None).unwrap();
        let nibble = bytes[6] >> 4;
        if nibble == 0 || nibble >= 6 {
            saw_non_rfc = true;
        }
    }
    assert!(
        saw_non_rfc,
        "every version nibble was in 1..=5 across 200 draws — \
         port is restricting the version field rather than passing through random bits"
    );
}

#[test]
fn generate_uuid_never_produces_nil() {
    let mut ntc = NativeTestCase::for_simplest(1000);
    let bytes = generate_uuid(&mut ntc, None).unwrap();
    assert_ne!(bytes, [0u8; 16], "nil UUID must never be produced");
}

#[test]
fn generate_uuid_rejects_wide_version() {
    let mut ntc = fresh_ntc(0);
    let err = generate_uuid(&mut ntc, Some(16)).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("hex nibble"));
}

#[test]
fn generate_ipv4_addresses_are_valid_and_hit_special_ranges() {
    let addrs: Vec<Ipv4Addr> = (0..200)
        .map(|seed| {
            let mut ntc = fresh_ntc(seed);
            generate_ipv4(&mut ntc).unwrap()
        })
        .collect();

    let saw_loopback = addrs.iter().any(|a| a.octets()[0] == 127);
    let saw_private_10 = addrs.iter().any(|a| a.octets()[0] == 10);
    let saw_192_168 = addrs.iter().any(|a| {
        let o = a.octets();
        o[0] == 192 && o[1] == 168
    });
    assert!(
        saw_loopback,
        "no 127.x.x.x address in 200 draws — special-range biasing missing"
    );
    assert!(saw_private_10, "no 10.x.x.x address in 200 draws");
    assert!(saw_192_168, "no 192.168.x.x address in 200 draws");
}

#[test]
fn generate_ipv6_addresses_are_valid_and_hit_special_ranges() {
    let addrs: Vec<Ipv6Addr> = (0..200)
        .map(|seed| {
            let mut ntc = fresh_ntc(seed);
            generate_ipv6(&mut ntc).unwrap()
        })
        .collect();

    let saw_loopback_or_unspecified = addrs
        .iter()
        .any(|a| *a == Ipv6Addr::LOCALHOST || *a == Ipv6Addr::UNSPECIFIED);
    let saw_doc = addrs.iter().any(|a| {
        let s = a.segments();
        s[0] == 0x2001 && s[1] == 0x0db8
    });
    assert!(
        saw_loopback_or_unspecified,
        "no ::1 / :: in 200 draws — special-range biasing missing"
    );
    assert!(saw_doc, "no 2001:db8::/32 address in 200 draws");
}

#[test]
fn civil_day_conversions_round_trip() {
    assert_eq!(days_from_civil(&date(1970, 1, 1)), 0);
    assert_eq!(days_from_civil(&date(2000, 1, 1)), 10_957);
    for days in (-800_000..800_000).step_by(37) {
        let d = civil_from_days(days);
        assert_eq!(days_from_civil(&d), days, "round trip failed for {d:?}");
        assert!((1..=12).contains(&d.month));
        assert!(d.day >= 1 && i64::from(d.day) <= days_in_month(d.year, d.month));
    }
    for (y, m, dd) in [(1, 1, 1), (1600, 2, 29), (1999, 12, 31), (9999, 12, 31)] {
        let d = date(y, m, dd);
        assert_eq!(civil_from_days(days_from_civil(&d)), d);
    }
}

#[test]
fn generate_date_respects_bounds() {
    let min = date(1990, 6, 15);
    let max = date(2010, 2, 3);
    for seed in 0..300 {
        let mut ntc = fresh_ntc(seed);
        let d = generate_date(&mut ntc, min, max).unwrap();
        let days = days_from_civil(&d);
        assert!(
            (days_from_civil(&min)..=days_from_civil(&max)).contains(&days),
            "{d:?} outside bounds"
        );
    }
}

#[test]
fn generate_date_shrinks_toward_2000_01_01_clamped() {
    use crate::native::core::ChoiceValue;
    for (min, max, expect) in [
        (date(1990, 1, 1), date(2010, 12, 31), date(2000, 1, 1)),
        (date(2005, 3, 2), date(2010, 12, 31), date(2005, 3, 2)),
        (date(1980, 1, 1), date(1990, 6, 6), date(1990, 6, 6)),
    ] {
        let mut ntc =
            NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(0))], None, None);
        let d = generate_date(&mut ntc, min, max).unwrap();
        assert_eq!(d, expect);
    }
}

#[test]
fn generate_date_rejects_invalid_arguments() {
    let mut ntc = fresh_ntc(0);
    for (min, max) in [
        (date(2000, 2, 30), FULL_MAX_DATE),
        (FULL_MIN_DATE, date(2000, 13, 1)),
        (date(2000, 0, 1), FULL_MAX_DATE),
        (date(2000, 1, 0), FULL_MAX_DATE),
        (date(1_000_000, 1, 1), date(1_000_000, 1, 2)),
        (date(2001, 1, 1), date(2000, 1, 1)),
    ] {
        let err = generate_date(&mut ntc, min, max).unwrap_err();
        assert!(
            matches!(err, EngineError::InvalidArgument(_)),
            "{min:?}..{max:?}"
        );
    }
}

#[test]
fn generate_date_supports_negative_years() {
    let min = date(-262_143, 1, 1);
    let max = date(262_143, 12, 31);
    let mut seen_negative = false;
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let d = generate_date(&mut ntc, min, max).unwrap();
        assert!((-262_143..=262_143).contains(&d.year));
        if d.year < 0 {
            seen_negative = true;
        }
    }
    assert!(seen_negative, "no negative-year dates across 200 seeds");
}

#[test]
fn generate_time_respects_bounds_and_shrinks_to_min() {
    use crate::native::core::ChoiceValue;
    let min = time(9, 30, 0, 0);
    let max = time(17, 0, 0, 0);
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let t = generate_time(&mut ntc, min, max).unwrap();
        let us = time_to_us(&t);
        assert!((time_to_us(&min)..=time_to_us(&max)).contains(&us), "{t:?}");
    }
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Integer(BigInt::from(0))], None, None);
    assert_eq!(generate_time(&mut ntc, min, max).unwrap(), min);
}

#[test]
fn generate_time_rejects_invalid_arguments() {
    let mut ntc = fresh_ntc(0);
    for (min, max) in [
        (time(24, 0, 0, 0), LAST_MICROSECOND),
        (MIDNIGHT, time(0, 60, 0, 0)),
        (MIDNIGHT, time(0, 0, 60, 0)),
        (MIDNIGHT, time(0, 0, 0, 1_000_000)),
        (time(1, 0, 0, 0), time(0, 59, 0, 0)),
    ] {
        let err = generate_time(&mut ntc, min, max).unwrap_err();
        assert!(
            matches!(err, EngineError::InvalidArgument(_)),
            "{min:?}..{max:?}"
        );
    }
}

#[test]
fn generate_datetime_respects_time_bounds_on_boundary_dates() {
    let min = DateTime {
        date: date(2000, 1, 1),
        time: time(12, 0, 0, 0),
    };
    let max = DateTime {
        date: date(2000, 1, 3),
        time: time(6, 0, 0, 0),
    };
    let mut seen_min_date = false;
    let mut seen_max_date = false;
    for seed in 0..500 {
        let mut ntc = fresh_ntc(seed);
        let dt = generate_datetime(&mut ntc, min, max).unwrap();
        let day = days_from_civil(&dt.date);
        assert!((days_from_civil(&min.date)..=days_from_civil(&max.date)).contains(&day));
        if day == days_from_civil(&min.date) {
            seen_min_date = true;
            assert!(time_to_us(&dt.time) >= time_to_us(&min.time), "{dt:?}");
        }
        if day == days_from_civil(&max.date) {
            seen_max_date = true;
            assert!(time_to_us(&dt.time) <= time_to_us(&max.time), "{dt:?}");
        }
    }
    assert!(seen_min_date && seen_max_date, "boundary dates not covered");
}

#[test]
fn generate_datetime_single_day_constrains_both_ends() {
    let min = DateTime {
        date: date(2020, 5, 5),
        time: time(10, 0, 0, 0),
    };
    let max = DateTime {
        date: date(2020, 5, 5),
        time: time(11, 0, 0, 0),
    };
    for seed in 0..100 {
        let mut ntc = fresh_ntc(seed);
        let dt = generate_datetime(&mut ntc, min, max).unwrap();
        assert_eq!(dt.date, min.date);
        let us = time_to_us(&dt.time);
        assert!((time_to_us(&min.time)..=time_to_us(&max.time)).contains(&us));
    }
}

#[test]
fn generate_datetime_rejects_inverted_bounds() {
    let mut ntc = fresh_ntc(0);
    let lo = DateTime {
        date: date(2020, 5, 5),
        time: time(11, 0, 0, 0),
    };
    let hi = DateTime {
        date: date(2020, 5, 5),
        time: time(10, 0, 0, 0),
    };
    let err = generate_datetime(&mut ntc, lo, hi).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
}
