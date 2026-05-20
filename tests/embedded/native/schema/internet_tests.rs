// Embedded tests for src/native/schema/internet.rs — drive each interpreter
// across many seeds and assert structural invariants. The integration tests
// in tests/test_strings.rs and tests/test_standard_generators.rs cover the
// same interpreters via the user-facing API; these tests exercise the
// private helpers (TLD list, draw_dns_label, url_encode_path) directly.

use rand::SeedableRng;
use rand::rngs::SmallRng;

use super::*;
use crate::cbor_utils::cbor_map;
use crate::native::core::NativeTestCase;

fn fresh_ntc(seed: u64) -> NativeTestCase {
    NativeTestCase::new_random(SmallRng::seed_from_u64(seed))
}

fn decode_string(v: ciborium::Value) -> String {
    let ciborium::Value::Tag(91, inner) = v else {
        panic!("expected tag-91 string, got {v:?}")
    };
    let ciborium::Value::Bytes(bytes) = *inner else {
        panic!("expected bytes inside tag-91")
    };
    String::from_utf8(bytes).unwrap()
}

fn domain_schema(max_length: u64) -> ciborium::Value {
    cbor_map! {"type" => "domain", "max_length" => max_length}
}

// ── TLD list ─────────────────────────────────────────────────────────────────

#[test]
fn top_level_domains_excludes_arpa() {
    assert!(
        !TOP_LEVEL_DOMAINS.contains(&"ARPA"),
        "ARPA must be filtered out per RFC 3172 — Hypothesis does the same"
    );
}

#[test]
fn top_level_domains_starts_with_com() {
    assert_eq!(
        TOP_LEVEL_DOMAINS[0], "COM",
        "COM is the shrink target, must be first"
    );
}

#[test]
fn top_level_domains_is_non_trivial() {
    // ~1500 entries from the IANA list (1438 file lines minus header minus
    // ARPA, plus the COM front-pin).
    assert!(
        TOP_LEVEL_DOMAINS.len() > 1000,
        "expected ~1500 TLDs, got {}",
        TOP_LEVEL_DOMAINS.len()
    );
}

// ── interpret_domain ─────────────────────────────────────────────────────────

#[test]
fn interpret_domain_default_max_length() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let schema = cbor_map! {"type" => "domain"};
        let s = decode_string(interpret_domain(&mut ntc, &schema).ok().unwrap());
        assert!(s.len() <= 255, "default max_length is 255: got {s:?}");
        assert!(s.contains('.'), "domain must have ≥ 2 labels: {s:?}");
        for part in s.split('.') {
            assert!(!part.is_empty(), "empty label in {s:?}");
            assert!(
                part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "non-DNS char in {s:?}"
            );
        }
    }
}

#[test]
fn interpret_domain_respects_max_length_4() {
    // The smallest configurable max_length. Most draws give the dotted form
    // (e.g. "x.aa"); some draw a too-long label first and break before
    // appending, in which case the output is just the TLD ("aa"). Both
    // shapes are produced by Hypothesis too and accepted here — the only
    // hard invariant is the length cap.
    let mut saw_dotted = false;
    for seed in 0..500 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_domain(&mut ntc, &domain_schema(4)).ok().unwrap());
        assert!(s.len() <= 4, "max_length=4 violated by {s:?}");
        if s.contains('.') {
            saw_dotted = true;
        }
    }
    assert!(
        saw_dotted,
        "expected to see at least one dotted draw across 500 seeds"
    );
}

#[test]
fn interpret_domain_respects_max_length_8() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_domain(&mut ntc, &domain_schema(8)).ok().unwrap());
        assert!(s.len() <= 8, "max_length=8 violated by {s:?}");
    }
}

#[test]
fn interpret_domain_labels_obey_rfc1035() {
    // Labels start with a letter, end with alphanumeric, contain only
    // letters/digits/hyphens, length ≤ 63.
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(
            interpret_domain(&mut ntc, &domain_schema(255))
                .ok()
                .unwrap(),
        );
        // The last label is the TLD (IANA list — all letters, may be mixed-case).
        // The earlier labels are RFC 1035 labels.
        let labels: Vec<&str> = s.split('.').collect();
        for label in &labels[..labels.len() - 1] {
            assert!(!label.is_empty() && label.len() <= 63, "label {label:?}");
            let bytes = label.as_bytes();
            assert!(
                bytes[0].is_ascii_alphabetic(),
                "label {label:?} must start with a letter"
            );
            assert!(
                bytes[bytes.len() - 1].is_ascii_alphanumeric(),
                "label {label:?} must end with alphanumeric"
            );
            assert!(
                bytes
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'-'),
                "label {label:?} has non-DNS chars"
            );
            assert!(
                label.len() < 4 || &label[2..4] != "--",
                "RFC 5890 reserved `xx--` prefix in {label:?}"
            );
        }
        // TLD comes from the IANA list (which includes XN-- punycode TLDs)
        // and is randomly recased. So digits, hyphens, and either case of
        // ASCII letter are all valid.
        let tld = labels[labels.len() - 1];
        assert!(
            tld.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "TLD {tld:?} has non-DNS chars"
        );
    }
}

#[test]
fn interpret_domain_recases_tld() {
    // Across 200 seeds we should see at least one all-lower and one all-upper TLD.
    let mut saw_lower = false;
    let mut saw_upper = false;
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(
            interpret_domain(&mut ntc, &domain_schema(255))
                .ok()
                .unwrap(),
        );
        let tld = s.rsplit('.').next().unwrap();
        if tld.chars().all(|c| c.is_ascii_lowercase()) {
            saw_lower = true;
        }
        if tld.chars().all(|c| c.is_ascii_uppercase()) {
            saw_upper = true;
        }
    }
    assert!(saw_lower, "expected to see at least one all-lowercase TLD");
    assert!(saw_upper, "expected to see at least one all-uppercase TLD");
}

// ── interpret_email ──────────────────────────────────────────────────────────

#[test]
fn interpret_email_has_one_at_sign() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let Ok(v) = interpret_email(&mut ntc) else {
            continue; // length filter rejection
        };
        let s = decode_string(v);
        let parts: Vec<&str> = s.split('@').collect();
        assert_eq!(parts.len(), 2, "expected exactly one '@' in {s:?}");
        let (local, domain) = (parts[0], parts[1]);
        assert!(!local.is_empty(), "empty local in {s:?}");
        assert!(domain.contains('.'), "domain {domain:?} lacks dot");
        assert!(s.len() <= 254, "RFC 5321 length violated: {s:?}");
    }
}

#[test]
fn interpret_email_local_part_in_atext_set() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let Ok(v) = interpret_email(&mut ntc) else {
            continue;
        };
        let s = decode_string(v);
        let (local, _) = s.split_once('@').unwrap();
        for c in local.chars() {
            let allowed = c.is_ascii_alphanumeric() || "!#$%&'*+-/=^_`{|}~".contains(c);
            assert!(allowed, "local-part char {c:?} not in atext set ({s:?})");
        }
    }
}

#[test]
fn interpret_email_never_ends_with_arpa() {
    // ARPA was filtered out of TOP_LEVEL_DOMAINS; verify nothing leaks.
    for seed in 0..500 {
        let mut ntc = fresh_ntc(seed);
        let Ok(v) = interpret_email(&mut ntc) else {
            continue;
        };
        let s = decode_string(v);
        assert!(!s.to_lowercase().ends_with(".arpa"), "ARPA leaked in {s:?}");
    }
}

// ── interpret_url ────────────────────────────────────────────────────────────

#[test]
fn interpret_url_has_http_scheme_and_authority() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_url(&mut ntc).ok().unwrap());
        assert!(
            s.starts_with("http://") || s.starts_with("https://"),
            "wrong scheme in {s:?}"
        );
        let after_scheme = s.split_once("://").unwrap().1;
        assert!(after_scheme.contains('/'), "missing path-separator: {s:?}");
    }
}

#[test]
fn interpret_url_path_chars_url_safe() {
    // url_encode_path keeps chars in URL_SAFE_CHARACTERS ∪ {%, digits}.
    // The full URL alphabet for the path is letters, digits, $-_.+!*'(),~%/
    let url_safe: std::collections::HashSet<char> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain("$-_.+!*'(),~%/".chars())
        .collect();
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_url(&mut ntc).ok().unwrap());
        let after_scheme = s.split_once("://").unwrap().1;
        let domain_path = after_scheme.split_once('#').map_or(after_scheme, |x| x.0);
        let path = domain_path.split_once('/').map_or("", |x| x.1);
        for c in path.chars() {
            assert!(
                url_safe.contains(&c),
                "path char {c:?} not URL-safe in {s:?}"
            );
        }
        // Every `%` must be followed by two hex chars.
        for chunk in path.split('%').skip(1) {
            let bytes = chunk.as_bytes();
            assert!(
                bytes.len() >= 2
                    && (bytes[0] as char).is_ascii_hexdigit()
                    && (bytes[1] as char).is_ascii_hexdigit(),
                "bad % escape in {s:?}: {chunk:?}"
            );
        }
    }
}

#[test]
fn interpret_url_fragments_chars_in_safe_set() {
    let frag_safe: std::collections::HashSet<char> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain("$-_.+!*'(),~%/?".chars())
        .collect();
    for seed in 0..300 {
        let mut ntc = fresh_ntc(seed);
        let s = decode_string(interpret_url(&mut ntc).ok().unwrap());
        let Some((_, fragment)) = s.split_once('#') else {
            continue;
        };
        for c in fragment.chars() {
            assert!(
                frag_safe.contains(&c),
                "fragment char {c:?} not in fragment-safe set ({s:?})"
            );
        }
    }
}

// ── url_encode_path edge cases ───────────────────────────────────────────────

#[test]
fn url_encode_path_safe_chars_passthrough() {
    assert_eq!(
        url_encode_path("abcXYZ012$-_.+!*'(),~"),
        "abcXYZ012$-_.+!*'(),~"
    );
}

#[test]
fn url_encode_path_encodes_space_and_slash() {
    // Space (32) and `/` (47, not url-safe in a single component since `/`
    // is the component separator) get percent-encoded.
    assert_eq!(url_encode_path("a b/c"), "a%20b%2Fc");
}

#[test]
fn url_encode_path_encodes_control_chars() {
    // \t (9) and \n (10) are in the path alphabet (string.printable) but
    // not URL-safe — they should be percent-encoded.
    assert_eq!(url_encode_path("\t\n"), "%09%0A");
}

#[test]
fn url_encode_path_encodes_high_latin1() {
    // The fragment alphabet covers codepoints up to 0xFF; the encoder
    // should emit %XX for those, with uppercase hex.
    let s: String = ['\u{00FF}', '\u{0080}'].iter().collect();
    assert_eq!(url_encode_path(&s), "%FF%80");
}
