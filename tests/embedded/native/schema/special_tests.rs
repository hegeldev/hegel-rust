// Embedded tests for src/native/schema/special.rs — drive each interpreter
// across many seeds and assert structural invariants. The integration tests
// in tests/test_strings.rs and tests/test_time.rs exercise the same
// interpreters via the user-facing API, but at coarser granularity.

use std::net::{Ipv4Addr, Ipv6Addr};

use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::cbor_utils::cbor_map;
use crate::native::core::NativeTestCase;

fn fresh_ntc(seed: u64) -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(seed))
}

fn decode_string(v: ciborium::Value) -> String {
    // The interpreters wrap strings in tag-91 (HEGEL_STRING_TAG). Match the
    // `deserialize_value` path in src/test_case.rs.
    let ciborium::Value::Tag(91, inner) = v else {
        panic!("expected tag-91 string, got {v:?}")
    };
    let ciborium::Value::Bytes(bytes) = *inner else {
        panic!("expected bytes inside tag-91")
    };
    String::from_utf8(bytes).unwrap()
}

/// Run an interpreter across many seeds, collecting the decoded string from
/// each successful draw. Useful for asserting distributional properties
/// (e.g. "at least one draw is a 127.x.x.x address").
fn collect<F>(n: u64, mut f: F) -> Vec<String>
where
    F: FnMut(&mut NativeTestCase) -> Result<ciborium::Value, crate::native::core::StopTest>,
{
    (0..n)
        .filter_map(|seed| {
            let mut ntc = fresh_ntc(seed);
            f(&mut ntc).ok().map(decode_string)
        })
        .collect()
}

// ── interpret_date ───────────────────────────────────────────────────────────

#[test]
fn interpret_date_produces_iso_format() {
    for seed in 0..50 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_date(&mut ntc).ok().unwrap());
        // YYYY-MM-DD (10 chars) and parses as date components in valid ranges.
        assert_eq!(s.len(), 10, "wrong length for {s:?}");
        let parts: Vec<&str> = s.split('-').collect();
        assert_eq!(parts.len(), 3, "{s:?}");
        let year: i32 = parts[0].parse().unwrap();
        let month: u32 = parts[1].parse().unwrap();
        let day: u32 = parts[2].parse().unwrap();
        assert!((1..=9999).contains(&year), "year out of range in {s:?}");
        assert!((1..=12).contains(&month), "month out of range in {s:?}");
        assert!((1..=31).contains(&day), "day out of range in {s:?}");
    }
}

#[test]
fn interpret_date_day_respects_month_length() {
    // Feb 30 must never appear: day is drawn against days_in_month(year, month).
    let mut seen_feb = false;
    for seed in 0..1000 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_date(&mut ntc).ok().unwrap());
        let parts: Vec<&str> = s.split('-').collect();
        let year: i128 = parts[0].parse().unwrap();
        let month: i128 = parts[1].parse().unwrap();
        let day: i128 = parts[2].parse().unwrap();
        if month == 2 {
            seen_feb = true;
            let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let max_day = if is_leap { 29 } else { 28 };
            assert!(
                day <= max_day,
                "Feb day {day} exceeds {max_day} in year {year} ({s:?})"
            );
        }
        if matches!(month, 4 | 6 | 9 | 11) {
            assert!(day <= 30, "30+ day in 30-day month: {s:?}");
        }
    }
    assert!(seen_feb, "no February dates drawn across 1000 seeds");
}

// ── interpret_time ───────────────────────────────────────────────────────────

#[test]
fn interpret_time_format_omits_microseconds_when_zero() {
    // Force microsecond to 0 by running until we get one (high prob given
    // biased_integer_sample favours boundary values).
    let mut seen_microsecond_zero = false;
    let mut seen_microsecond_nonzero = false;
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_time(&mut ntc).ok().unwrap());
        match s.len() {
            8 => seen_microsecond_zero = true,     // HH:MM:SS
            15 => seen_microsecond_nonzero = true, // HH:MM:SS.ffffff
            _ => panic!("unexpected time format: {s:?}"),
        }
        // Components in valid ranges.
        let head: &str = s.split('.').next().unwrap();
        let parts: Vec<&str> = head.split(':').collect();
        let hour: u32 = parts[0].parse().unwrap();
        let minute: u32 = parts[1].parse().unwrap();
        let second: u32 = parts[2].parse().unwrap();
        assert!(hour <= 23 && minute <= 59 && second <= 59, "{s:?}");
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

// ── interpret_datetime ───────────────────────────────────────────────────────

#[test]
fn interpret_datetime_format_has_t_separator() {
    for seed in 0..50 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_datetime(&mut ntc).ok().unwrap());
        let parts: Vec<&str> = s.splitn(2, 'T').collect();
        assert_eq!(parts.len(), 2, "missing T separator in {s:?}");
        assert_eq!(parts[0].len(), 10, "date part wrong length in {s:?}");
        // Time part: 8 chars (HH:MM:SS) or 15 chars (HH:MM:SS.ffffff).
        assert!(
            matches!(parts[1].len(), 8 | 15),
            "time part wrong length in {s:?}"
        );
    }
}

// ── interpret_uuid ───────────────────────────────────────────────────────────

#[test]
fn interpret_uuid_default_format() {
    for seed in 0..50 {
        let mut ntc = fresh_ntc(seed);
        let schema = cbor_map! { "type" => "uuid" };
        let s = decode_string(interpret_uuid(&mut ntc, &schema).ok().unwrap());
        assert_eq!(s.len(), 36, "{s:?}");
        // Hyphens at positions 8, 13, 18, 23.
        for &pos in &[8usize, 13, 18, 23] {
            assert_eq!(s.as_bytes()[pos], b'-', "missing hyphen at {pos} in {s:?}");
        }
        // Everything else is a lowercase hex digit.
        for (i, b) in s.bytes().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                continue;
            }
            assert!(
                b.is_ascii_hexdigit() && !b.is_ascii_uppercase(),
                "non-hex / non-lowercase byte {b:#x} at {i} in {s:?}"
            );
        }
    }
}

#[test]
fn interpret_uuid_respects_version_field() {
    for version in 1u8..=5 {
        let schema = cbor_map! { "type" => "uuid", "version" => u64::from(version) };
        for seed in 0..30 {
            let mut ntc = fresh_ntc(seed);
            let s = decode_string(interpret_uuid(&mut ntc, &schema).ok().unwrap());
            // Version nibble is at index 14.
            let v_hex = s.as_bytes()[14];
            let v_digit = (v_hex as char).to_digit(16).unwrap() as u8;
            assert_eq!(v_digit, version, "version mismatch in {s:?}");
            // Variant nibble at index 19 must be one of 8, 9, a, b.
            let var_hex = s.as_bytes()[19];
            let var_digit = (var_hex as char).to_digit(16).unwrap();
            assert!(
                matches!(var_digit, 0x8..=0xb),
                "variant nibble {var_digit:x} in {s:?} not in 8..=b"
            );
        }
    }
}

// ── interpret_ip_address ─────────────────────────────────────────────────────

#[test]
fn interpret_ipv4_parses_back() {
    let schema = cbor_map! { "type" => "ip_address", "version" => 4u64 };
    for seed in 0..100 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_ip_address(&mut ntc, &schema).ok().unwrap());
        s.parse::<Ipv4Addr>()
            .unwrap_or_else(|e| panic!("invalid IPv4 {s:?}: {e}"));
    }
}

#[test]
fn interpret_ipv6_parses_back() {
    let schema = cbor_map! { "type" => "ip_address", "version" => 6u64 };
    for seed in 0..100 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_ip_address(&mut ntc, &schema).ok().unwrap());
        s.parse::<Ipv6Addr>()
            .unwrap_or_else(|e| panic!("invalid IPv6 {s:?}: {e}"));
    }
}

#[test]
fn interpret_ipv4_hits_special_ranges() {
    // With 50/50 between uniform and special-range, ~half of draws should
    // land inside one of the SPECIAL_IPV4_RANGES. We assert the much weaker
    // "at least one loopback, one private, and one documentation address
    // appears across 200 seeds" — vanishingly unlikely under pure uniform.
    let schema = cbor_map! { "type" => "ip_address", "version" => 4u64 };
    let addrs: Vec<Ipv4Addr> = (0..200)
        .map(|seed| {
            let mut ntc = fresh_ntc(seed);
            decode_string(interpret_ip_address(&mut ntc, &schema).ok().unwrap())
                .parse()
                .unwrap()
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
fn interpret_ipv6_hits_special_ranges() {
    let schema = cbor_map! { "type" => "ip_address", "version" => 6u64 };
    let addrs: Vec<Ipv6Addr> = (0..200)
        .map(|seed| {
            let mut ntc = fresh_ntc(seed);
            decode_string(interpret_ip_address(&mut ntc, &schema).ok().unwrap())
                .parse()
                .unwrap()
        })
        .collect();

    // Loopback (::1) or unspecified (::) — the /128 entries in SPECIAL_IPV6_RANGES.
    let saw_loopback_or_unspecified = addrs
        .iter()
        .any(|a| *a == Ipv6Addr::LOCALHOST || *a == Ipv6Addr::UNSPECIFIED);
    // Documentation prefix 2001:db8::/32.
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
#[should_panic(expected = "ip_address: unsupported version 5")]
fn interpret_ip_address_unknown_version_panics() {
    let mut ntc = fresh_ntc(0);
    let schema = cbor_map! { "type" => "ip_address", "version" => 5u64 };
    let _ = interpret_ip_address(&mut ntc, &schema);
}

#[test]
#[should_panic(expected = "ip_address schema must have a \"version\" field")]
fn interpret_ip_address_missing_version_panics() {
    let mut ntc = fresh_ntc(0);
    let schema = cbor_map! { "type" => "ip_address" };
    let _ = interpret_ip_address(&mut ntc, &schema);
}

// ── interpret_uuid: distribution across versions ─────────────────────────────

#[test]
fn interpret_uuid_default_can_produce_non_rfc_versions() {
    // Hypothesis's `uuids(version=None)` uses raw 128 random bits — every
    // version nibble in 0..=f is reachable. The agent's port restricted the
    // nibble to 1..=5; the Hypothesis-faithful port must not. Across 200
    // draws of a uniform 4-bit value, P(no nibble outside 1..=5) =
    // (5/16)^200 ≈ 0, so seeing one is essentially certain when the port
    // is correct.
    let schema = cbor_map! { "type" => "uuid" };
    let strings = collect(200, |ntc| interpret_uuid(ntc, &schema));
    let saw_non_rfc = strings.iter().any(|s| {
        let nibble = (s.as_bytes()[14] as char).to_digit(16).unwrap();
        nibble == 0 || nibble >= 6
    });
    assert!(
        saw_non_rfc,
        "every version nibble was in 1..=5 across 200 draws — \
         port is restricting the version field rather than passing through random bits"
    );
}
