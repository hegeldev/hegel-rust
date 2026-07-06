use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::*;
use crate::native::core::NativeTestCase;
use crate::native::rng::EngineRng;

fn fresh_ntc(seed: u64) -> NativeTestCase {
    NativeTestCase::new_random(EngineRng::seeded(seed))
}

#[test]
fn generate_date_produces_valid_dates() {
    for seed in 0..50 {
        let mut ntc = fresh_ntc(seed);
        let d = generate_date(&mut ntc).unwrap();
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
        let d = generate_date(&mut ntc).unwrap();
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
        let t = generate_time(&mut ntc).unwrap();
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
        let dt = generate_datetime(&mut ntc).unwrap();
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
            match generate_ip_address(&mut ntc, 4).unwrap() {
                IpAddr::V4(a) => a,
                other => panic!("expected v4, got {other:?}"),
            }
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
            match generate_ip_address(&mut ntc, 6).unwrap() {
                IpAddr::V6(a) => a,
                other => panic!("expected v6, got {other:?}"),
            }
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
fn generate_ip_address_unknown_version_is_invalid_argument() {
    let mut ntc = fresh_ntc(0);
    let err = generate_ip_address(&mut ntc, 5).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
    assert!(err.to_string().contains("unsupported version 5"));
}
