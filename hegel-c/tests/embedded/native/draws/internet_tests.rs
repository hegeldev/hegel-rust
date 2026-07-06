use super::*;
use crate::native::core::NativeTestCase;
use crate::native::rng::EngineRng;

fn fresh_ntc(seed: u64) -> NativeTestCase {
    NativeTestCase::new_random(EngineRng::seeded(seed))
}

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
    assert!(
        TOP_LEVEL_DOMAINS.len() > 1000,
        "expected ~1500 TLDs, got {}",
        TOP_LEVEL_DOMAINS.len()
    );
}

#[test]
fn generate_domain_default_max_length() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_domain(&mut ntc, 255).unwrap();
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
fn generate_domain_respects_max_length_4() {
    let mut saw_dotted = false;
    for seed in 0..500 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_domain(&mut ntc, 4).unwrap();
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
fn generate_domain_respects_max_length_8() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_domain(&mut ntc, 8).unwrap();
        assert!(s.len() <= 8, "max_length=8 violated by {s:?}");
    }
}

#[test]
fn generate_domain_labels_obey_rfc1035() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_domain(&mut ntc, 255).unwrap();
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
        let tld = labels[labels.len() - 1];
        assert!(
            tld.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "TLD {tld:?} has non-DNS chars"
        );
    }
}

#[test]
fn generate_domain_recases_tld() {
    let mut saw_lower = false;
    let mut saw_upper = false;
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_domain(&mut ntc, 255).unwrap();
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

#[test]
fn generate_email_has_one_at_sign() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let Ok(s) = generate_email(&mut ntc) else {
            continue;
        };
        let parts: Vec<&str> = s.split('@').collect();
        assert_eq!(parts.len(), 2, "expected exactly one '@' in {s:?}");
        let (local, domain) = (parts[0], parts[1]);
        assert!(!local.is_empty(), "empty local in {s:?}");
        assert!(domain.contains('.'), "domain {domain:?} lacks dot");
        assert!(s.len() <= 254, "RFC 5321 length violated: {s:?}");
    }
}

#[test]
fn generate_email_local_part_in_atext_set() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let Ok(s) = generate_email(&mut ntc) else {
            continue;
        };
        let (local, _) = s.split_once('@').unwrap();
        for c in local.chars() {
            let allowed = c.is_ascii_alphanumeric() || "!#$%&'*+-/=^_`{|}~".contains(c);
            assert!(allowed, "local-part char {c:?} not in atext set ({s:?})");
        }
    }
}

#[test]
fn generate_email_never_ends_with_arpa() {
    for seed in 0..500 {
        let mut ntc = fresh_ntc(seed);
        let Ok(s) = generate_email(&mut ntc) else {
            continue;
        };
        assert!(!s.to_lowercase().ends_with(".arpa"), "ARPA leaked in {s:?}");
    }
}

#[test]
fn generate_url_has_http_scheme_and_authority() {
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_url(&mut ntc).unwrap();
        assert!(
            s.starts_with("http://") || s.starts_with("https://"),
            "wrong scheme in {s:?}"
        );
        let after_scheme = s.split_once("://").unwrap().1;
        assert!(after_scheme.contains('/'), "missing path-separator: {s:?}");
    }
}

#[test]
fn generate_url_path_chars_url_safe() {
    let url_safe: std::collections::HashSet<char> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain("$-_.+!*'(),~%/".chars())
        .collect();
    for seed in 0..200 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_url(&mut ntc).unwrap();
        let after_scheme = s.split_once("://").unwrap().1;
        let domain_path = after_scheme.split_once('#').map_or(after_scheme, |x| x.0);
        let path = domain_path.split_once('/').map_or("", |x| x.1);
        for c in path.chars() {
            assert!(
                url_safe.contains(&c),
                "path char {c:?} not URL-safe in {s:?}"
            );
        }
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
fn generate_url_fragments_chars_in_safe_set() {
    let frag_safe: std::collections::HashSet<char> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain("$-_.+!*'(),~%/?".chars())
        .collect();
    for seed in 0..300 {
        let mut ntc = fresh_ntc(seed);
        let s = generate_url(&mut ntc).unwrap();
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

#[test]
fn url_encode_path_safe_chars_passthrough() {
    assert_eq!(
        url_encode_path("abcXYZ012$-_.+!*'(),~"),
        "abcXYZ012$-_.+!*'(),~"
    );
}

#[test]
fn url_encode_path_encodes_space_and_slash() {
    assert_eq!(url_encode_path("a b/c"), "a%20b%2Fc");
}

#[test]
fn url_encode_path_encodes_control_chars() {
    assert_eq!(url_encode_path("\t\n"), "%09%0A");
}

#[test]
fn url_encode_path_encodes_high_latin1() {
    let s: String = ['\u{00FF}', '\u{0080}'].iter().collect();
    assert_eq!(url_encode_path(&s), "%FF%80");
}

#[test]
fn generate_domain_rejects_tiny_max_length_as_invalid_argument() {
    let mut ntc = fresh_ntc(0);
    let err = generate_domain(&mut ntc, 3).unwrap_err();
    assert!(matches!(err, EngineError::InvalidArgument(_)));
}

#[test]
fn validate_domain_max_length_matches_generate() {
    assert!(validate_domain_max_length(3).is_err());
    assert!(validate_domain_max_length(4).is_ok());
    assert!(validate_domain_max_length(255).is_ok());
}

#[test]
fn domain_max_length_above_255_is_an_invalid_argument() {
    for max_length in [256, 100_000] {
        let err = validate_domain_max_length(max_length).unwrap_err();
        let EngineError::InvalidArgument(msg) = err else {
            panic!("expected InvalidArgument, got {err:?}");
        };
        assert!(msg.contains("255"), "unexpected message: {msg}");

        let mut ntc = fresh_ntc(0);
        let err = generate_domain(&mut ntc, max_length).unwrap_err();
        assert!(matches!(err, EngineError::InvalidArgument(_)));
    }
}
