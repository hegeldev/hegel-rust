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
fn interpret_domain_short_max_length_disables_subdomains() {
    // max_length < 10 forces max_subs = 0 → exactly SLD.TLD.
    let mut ntc = fresh_ntc();
    let schema = cbor_map! {
        "type" => "domain",
        "max_length" => 9u64,
    };
    let s = decode_tagged(&interpret_domain(&mut ntc, &schema).ok().unwrap());
    let labels: Vec<&str> = s.split('.').collect();
    assert_eq!(labels.len(), 2);
}

#[test]
fn interpret_domain_with_two_subdomains() {
    // Force n_subs = 2 by replaying choices: first integer drawn from
    // [0, max_subs] is the subdomain count.
    let mut ntc = NativeTestCase::for_choices(
        &[
            ChoiceValue::Integer(2), // n_subs = 2
            ChoiceValue::Integer(3), // sub1 length
            ChoiceValue::Integer(0), // sub1 letter
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(3), // sub2 length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(3), // SLD length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(2), // TLD length
            ChoiceValue::Integer(0),
            ChoiceValue::Integer(0),
        ],
        None,
        None,
    );
    let schema = cbor_map! { "type" => "domain" };
    let s = decode_tagged(&interpret_domain(&mut ntc, &schema).ok().unwrap());
    assert_eq!(s, "aaa.aaa.aaa.aa");
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
