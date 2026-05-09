// Embedded tests for src/native/schema/special.rs — exercise each
// interpret_* function with a deterministic NativeTestCase. Tests check the
// output format and exercise branches inside the interpreters.

use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::cbor_utils::cbor_map;
use crate::native::core::{ChoiceValue, NativeTestCase};

fn fresh_ntc() -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(7))
}

/// Decode a string produced by `encode_string` (CBOR tag 91 wrapping bytes).
fn decode_tagged(value: &Value) -> String {
    let Value::Tag(91, boxed) = value else {
        panic!("expected tag 91, got {:?}", value)
    };
    let Value::Bytes(bytes) = boxed.as_ref() else {
        panic!("expected bytes inside tag 91, got {:?}", boxed)
    };
    String::from_utf8(bytes.clone()).expect("bytes should be UTF-8")
}

// ── interpret_date ─────────────────────────────────────────────────────────

#[test]
fn interpret_date_produces_yyyy_mm_dd() {
    let mut ntc = fresh_ntc();
    let s = decode_tagged(&interpret_date(&mut ntc).ok().unwrap());
    assert_eq!(s.len(), 10);
    let parts: Vec<&str> = s.split('-').collect();
    assert_eq!(parts.len(), 3);
    let year: u32 = parts[0].parse().unwrap();
    let month: u32 = parts[1].parse().unwrap();
    let day: u32 = parts[2].parse().unwrap();
    assert!((1970..=2100).contains(&year));
    assert!((1..=12).contains(&month));
    assert!((1..=28).contains(&day));
}

// ── interpret_time ─────────────────────────────────────────────────────────

#[test]
fn interpret_time_produces_hh_mm_ss() {
    let mut ntc = fresh_ntc();
    let s = decode_tagged(&interpret_time(&mut ntc).ok().unwrap());
    assert_eq!(s.len(), 8);
    let parts: Vec<&str> = s.split(':').collect();
    assert_eq!(parts.len(), 3);
    let hour: u32 = parts[0].parse().unwrap();
    let minute: u32 = parts[1].parse().unwrap();
    let second: u32 = parts[2].parse().unwrap();
    assert!(hour <= 23);
    assert!(minute <= 59);
    assert!(second <= 59);
}

#[test]
fn interpret_time_midnight_is_reachable() {
    // Forcing total_secs = 0 should give "00:00:00".
    let mut ntc = NativeTestCase::for_choices(&[ChoiceValue::Integer(0)], None, None);
    let s = decode_tagged(&interpret_time(&mut ntc).ok().unwrap());
    assert_eq!(s, "00:00:00");
}

// ── interpret_datetime ─────────────────────────────────────────────────────

#[test]
fn interpret_datetime_produces_full_iso_format() {
    let mut ntc = fresh_ntc();
    let s = decode_tagged(&interpret_datetime(&mut ntc).ok().unwrap());
    assert_eq!(s.len(), 19);
    let (date, time) = s.split_once('T').expect("missing T separator");
    assert_eq!(date.len(), 10);
    assert_eq!(time.len(), 8);
    assert!(date.split('-').count() == 3);
    assert!(time.split(':').count() == 3);
}

// ── interpret_ipv4 ─────────────────────────────────────────────────────────

#[test]
fn interpret_ipv4_produces_dotted_octets() {
    let mut ntc = fresh_ntc();
    let s = decode_tagged(&interpret_ipv4(&mut ntc).ok().unwrap());
    let parts: Vec<&str> = s.split('.').collect();
    assert_eq!(parts.len(), 4);
    for p in parts {
        let n: u32 = p.parse().unwrap();
        assert!(n <= 255);
    }
}

// ── interpret_ipv6 ─────────────────────────────────────────────────────────

#[test]
fn interpret_ipv6_produces_eight_hex_groups() {
    let mut ntc = fresh_ntc();
    let s = decode_tagged(&interpret_ipv6(&mut ntc).ok().unwrap());
    let parts: Vec<&str> = s.split(':').collect();
    assert_eq!(parts.len(), 8);
    for p in parts {
        assert_eq!(p.len(), 4);
        let n = u32::from_str_radix(p, 16).unwrap();
        assert!(n <= 0xFFFF);
    }
}

// ── interpret_ip_address ───────────────────────────────────────────────────

#[test]
fn interpret_ip_address_v4_produces_dotted_octets() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "ip_address", "version" => 4u64 };
    let s = decode_tagged(&interpret_ip_address(&mut ntc, &schema).ok().unwrap());
    let parts: Vec<&str> = s.split('.').collect();
    assert_eq!(parts.len(), 4);
    for p in parts {
        let n: u32 = p.parse().unwrap();
        assert!(n <= 255);
    }
}

#[test]
fn interpret_ip_address_v6_produces_eight_hex_groups() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "ip_address", "version" => 6u64 };
    let s = decode_tagged(&interpret_ip_address(&mut ntc, &schema).ok().unwrap());
    let parts: Vec<&str> = s.split(':').collect();
    assert_eq!(parts.len(), 8);
    for p in parts {
        assert_eq!(p.len(), 4);
        let n = u32::from_str_radix(p, 16).unwrap();
        assert!(n <= 0xFFFF);
    }
}

// ── interpret_domain ───────────────────────────────────────────────────────

#[test]
fn interpret_domain_default_max_length_has_at_least_two_labels() {
    let mut ntc = fresh_ntc();
    let schema = cbor_map! { "type" => "domain" };
    let s = decode_tagged(&interpret_domain(&mut ntc, &schema).ok().unwrap());
    let labels: Vec<&str> = s.split('.').collect();
    assert!(labels.len() >= 2);
    // SLD is 3-8 letters, TLD is 2-4 letters.
    let tld = labels.last().unwrap();
    assert!((2..=4).contains(&tld.len()));
}

#[test]
fn interpret_domain_minimum_max_length_yields_two_labels() {
    // The smallest legal `max_length` is 4 (`DomainGenerator::build_schema`
    // asserts this). At 4 chars there's exactly enough room for a 1-letter
    // SLD + dot + 2-letter TLD ("a.aa"), which leaves no budget for any
    // subdomains: `interpret_domain` must produce exactly 2 labels.
    let schema = cbor_map! {
        "type" => "domain",
        "max_length" => 4u64,
    };
    for seed in 0u64..50 {
        let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
        let s = decode_tagged(&interpret_domain(&mut ntc, &schema).ok().unwrap());
        let labels: Vec<&str> = s.split('.').collect();
        assert_eq!(
            labels.len(),
            2,
            "max_length=4 should produce exactly SLD.TLD; got {s:?}"
        );
        assert!(s.len() <= 4, "max_length=4 violated by {s:?}");
    }
}

#[test]
fn interpret_domain_with_two_subdomains() {
    // Force n_subs = 2 by replaying the choice sequence emitted by the
    // new (post-A3) budget-driven layout: TLD len → TLD chars → SLD len
    // → SLD chars → n_subs → (sub_len, sub chars)*. Min-sized labels
    // throughout produce "a.a.a.aa".
    let mut ntc = NativeTestCase::for_choices(
        &[
            ChoiceValue::Integer(2), // TLD length
            ChoiceValue::Integer(0), // TLD char 0 ('a')
            ChoiceValue::Integer(0), // TLD char 1 ('a')
            ChoiceValue::Integer(1), // SLD length
            ChoiceValue::Integer(0), // SLD char ('a' — letter-only since len=1)
            ChoiceValue::Integer(2), // n_subs = 2
            ChoiceValue::Integer(1), // sub1 length
            ChoiceValue::Integer(0), // sub1 char
            ChoiceValue::Integer(1), // sub2 length
            ChoiceValue::Integer(0), // sub2 char
        ],
        None,
        None,
    );
    let schema = cbor_map! { "type" => "domain" };
    let s = decode_tagged(&interpret_domain(&mut ntc, &schema).ok().unwrap());
    assert_eq!(s, "a.a.a.aa");
}

// ── interpret_email ────────────────────────────────────────────────────────

#[test]
fn interpret_email_has_user_at_host_dot_tld() {
    let mut ntc = fresh_ntc();
    let s = decode_tagged(&interpret_email(&mut ntc).ok().unwrap());
    let (user, rest) = s.split_once('@').expect("missing @");
    assert!((3..=15).contains(&user.len()));
    let labels: Vec<&str> = rest.split('.').collect();
    assert_eq!(labels.len(), 2);
    assert!((3..=8).contains(&labels[0].len()));
    assert!((2..=4).contains(&labels[1].len()));
}

// ── interpret_url ──────────────────────────────────────────────────────────

#[test]
fn interpret_url_random_returns_valid_url() {
    let mut ntc = fresh_ntc();
    let s = decode_tagged(&interpret_url(&mut ntc).ok().unwrap());
    assert!(s.starts_with("http://") || s.starts_with("https://"));
    let after_scheme = s.split_once("://").unwrap().1;
    let (host, _path) = after_scheme.split_once('/').unwrap_or((after_scheme, ""));
    let labels: Vec<&str> = host.split('.').collect();
    assert_eq!(labels.len(), 2);
}

#[test]
fn interpret_url_https_with_no_path_components() {
    // Force use_https=true and n_components=0.
    let mut ntc = NativeTestCase::for_choices(
        &[
            ChoiceValue::Boolean(true), // use_https
            ChoiceValue::Integer(3),    // host_label length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(2), // tld length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0), // n_components
        ],
        None,
        None,
    );
    let s = decode_tagged(&interpret_url(&mut ntc).ok().unwrap());
    assert_eq!(s, "https://aaa.aa");
}

#[test]
fn interpret_url_http_with_path_components() {
    // Force use_https=false and n_components=2.
    let mut ntc = NativeTestCase::for_choices(
        &[
            ChoiceValue::Boolean(false), // use_https → http
            ChoiceValue::Integer(3),     // host length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(2), // tld length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(2), // n_components = 2
            ChoiceValue::Integer(2), // component 1 length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(2), // component 2 length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
        ],
        None,
        None,
    );
    let s = decode_tagged(&interpret_url(&mut ntc).ok().unwrap());
    assert_eq!(s, "http://aaa.aa/aa/aa");
}

// ── interpret_domain: max_length enforcement + RFC 1035 charset ────────────

/// `interpret_domain` must respect the schema's `max_length` for *every*
/// drawn output. The pre-A3 code chose `n_subs ∈ {0, 1, 2}` whenever
/// `max_length >= 10`, but each sub adds up to 9 chars plus a dot, so
/// `gs::domains().max_length(10)` could produce strings up to ~31 chars.
#[test]
fn interpret_domain_respects_max_length_across_seeds() {
    for max_length in [4u64, 6, 9, 10, 15, 20, 25, 50, 100, 255] {
        let schema = cbor_map! {
            "type" => "domain",
            "max_length" => max_length,
        };
        for seed in 0u64..200 {
            let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
            let s = decode_tagged(&interpret_domain(&mut ntc, &schema).ok().unwrap());
            assert!(
                s.len() as u64 <= max_length,
                "max_length={max_length} but got {s:?} of length {}",
                s.len()
            );
            assert!(
                !s.is_empty() && s.contains('.'),
                "domain {s:?} should be non-empty and contain a dot"
            );
        }
    }
}

/// Domain labels should follow RFC 1035: start with a letter, end with
/// a letter or digit, internal characters can be letters / digits /
/// hyphens. The pre-A3 implementation drew labels from the lowercase
/// letters only — no digits, no hyphens — which made
/// `gs::domains()` unable to surface bugs in code that handles
/// alphanumeric or hyphenated labels (very common in real DNS).
#[test]
fn interpret_domain_charset_includes_digits_and_hyphens() {
    let schema = cbor_map! { "type" => "domain", "max_length" => 60u64 };
    let mut saw_digit = false;
    let mut saw_hyphen = false;
    for seed in 0u64..1000 {
        let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
        let s = decode_tagged(&interpret_domain(&mut ntc, &schema).ok().unwrap());
        for label in s.split('.') {
            assert!(!label.is_empty(), "label in {s:?} is empty");
            for (i, c) in label.chars().enumerate() {
                let is_first = i == 0;
                let is_last = i == label.len() - 1;
                if is_first {
                    assert!(
                        c.is_ascii_alphabetic(),
                        "label {label:?} (in {s:?}) starts with non-letter {c:?}"
                    );
                } else if is_last {
                    assert!(
                        c.is_ascii_alphanumeric(),
                        "label {label:?} ends with {c:?} (must be letter or digit)"
                    );
                } else {
                    assert!(
                        c.is_ascii_alphanumeric() || c == '-',
                        "label {label:?} has invalid character {c:?}"
                    );
                }
                if c.is_ascii_digit() {
                    saw_digit = true;
                }
                if c == '-' {
                    saw_hyphen = true;
                }
            }
        }
    }
    assert!(
        saw_digit,
        "expected at least one drawn domain to contain a digit across 1000 seeds"
    );
    assert!(
        saw_hyphen,
        "expected at least one drawn domain to contain a hyphen across 1000 seeds"
    );
}

// ── interpret_uuid: version distribution when unspecified ──────────────────

/// `gs::uuids()` (no `.version(...)`) emits a schema without a `version`
/// field; the doc on `UuidsGenerator` advertises this as "UUIDs of any
/// version." `interpret_uuid` must therefore pick a version randomly,
/// not silently default to v4. We draw 1000 UUIDs across distinct seeds
/// and assert the version nibble (15th character of the canonical form,
/// i.e. the first nibble of the third hyphen-separated group) covers
/// at least three of the RFC 4122 versions {1, 2, 3, 4, 5}.
#[test]
fn interpret_uuid_no_version_varies_across_rfc_versions() {
    let schema = cbor_map! {};
    let mut versions: std::collections::HashSet<char> = std::collections::HashSet::new();
    for seed in 0u64..1000 {
        let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
        let s = decode_tagged(&interpret_uuid(&mut ntc, &schema).ok().unwrap());
        let groups: Vec<&str> = s.split('-').collect();
        let g3 = groups[2];
        let version_nibble = g3.chars().next().unwrap();
        assert!(
            matches!(version_nibble, '1' | '2' | '3' | '4' | '5'),
            "uuid {s:?} has a non-RFC-4122 version nibble {version_nibble:?}"
        );
        versions.insert(version_nibble);
    }
    assert!(
        versions.len() >= 3,
        "expected `gs::uuids()` (no version) to produce >=3 distinct RFC \
         versions across 1000 draws, got {versions:?}"
    );
}

// ── interpret_uuid: RFC 4122 variant nibble ────────────────────────────────

/// Every UUID produced by `interpret_uuid` must have RFC 4122-compliant
/// variant bits: the first nibble of the fourth hyphen-separated group
/// (`Nxxx`) must be one of `8`, `9`, `a`, or `b` (i.e. top two bits of
/// that 16-bit group are `10`).
///
/// Audit item A1 claimed that `g4 = g4_low | 0x8000` only forces the top
/// bit, leaving `N ∈ {8..f}`. That would be true if `g4_low` were a 16-bit
/// draw, but `interpret_uuid` constrains it to 14 bits via
/// `draw_integer(0, 0x3FFF)`, so bit 14 is always 0 and the OR with
/// `0x8000` produces a 16-bit value with top two bits `10`. This test
/// pins that invariant: 1000 random UUIDs all match `[8-b][0-9a-f]{3}`
/// at the variant nibble.
#[test]
fn interpret_uuid_variant_nibble_is_rfc4122() {
    let schema = cbor_map! {};
    let mut all_seen: std::collections::HashSet<char> = std::collections::HashSet::new();
    for seed in 0u64..1000 {
        let mut ntc = NativeTestCase::new_random(SmallRng::seed_from_u64(seed));
        let s = decode_tagged(&interpret_uuid(&mut ntc, &schema).ok().unwrap());
        // Format: xxxxxxxx-xxxx-Mxxx-Nxxx-xxxxxxxxxxxx
        let groups: Vec<&str> = s.split('-').collect();
        assert_eq!(groups.len(), 5, "uuid {s:?} has the wrong shape");
        let g4 = groups[3];
        assert_eq!(g4.len(), 4, "g4 group {g4:?} has the wrong length");
        let variant_nibble = g4.chars().next().unwrap();
        assert!(
            matches!(variant_nibble, '8' | '9' | 'a' | 'b'),
            "uuid {s:?} has a non-RFC-4122 variant nibble {variant_nibble:?} \
             (top two bits of g4 should be 10)"
        );
        all_seen.insert(variant_nibble);
    }
    // Sanity check: across 1000 UUIDs we expect to see at least two of the
    // four allowed variant nibbles (the draw is uniform over 14 bits, so
    // each nibble appears with ~25% probability).
    assert!(
        all_seen.len() >= 2,
        "expected variant nibble distribution to span >=2 of {{8,9,a,b}}, \
         got {all_seen:?}"
    );
}
