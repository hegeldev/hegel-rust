// Interpreters for the internet-flavoured string schemas: domain, email, url.
// All produce ciborium tag-91 strings matching the output of Hypothesis's
// `provisional.domains()` / `strategies.emails()` / `provisional.urls()`
// strategies.

use std::sync::LazyLock;

use crate::cbor_utils::{as_u64, map_get};
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{EngineError, ManyState, NativeTestCase, Status};
use crate::native::intervalsets::IntervalSet;
use ciborium::Value;

use super::many_more;
use crate::control::hegel_internal_debug_assert;

/// The IANA top-level-domain list, vendored from
/// `http://data.iana.org/TLD/tlds-alpha-by-domain.txt`. Same file Hypothesis
/// ships at `hypothesis/vendor/tlds-alpha-by-domain.txt`. One TLD per line,
/// uppercase, with a leading `#`-comment line carrying the version stamp.
const IANA_TLDS_TXT: &str = include_str!("tlds-alpha-by-domain.txt");

/// Eligible TLDs, with `ARPA` removed (RFC 3172 reserved infrastructure
/// domain — Hypothesis specifically drops it so generated addresses don't
/// look like `.in-addr.arpa` reverse-lookup names) and `COM` moved to the
/// front so the shrink target is `.com`. Mirrors
/// `hypothesis.provisional.get_top_level_domains`.
static TOP_LEVEL_DOMAINS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut sorted: Vec<&'static str> = IANA_TLDS_TXT
        .lines()
        .filter(|line| !line.starts_with('#') && *line != "ARPA")
        .collect();
    sorted.sort_by_key(|s| s.len());
    let mut result = Vec::with_capacity(sorted.len() + 1);
    result.push("COM");
    result.extend(sorted);
    result
});

/// RFC 5322 atext set used by Hypothesis's `emails()` for the local part:
/// `string.ascii_letters + string.digits + "!#$%&'*+-/=^_\`{|}~"`. Encoded
/// here as merged codepoint intervals in ascending order.
fn email_local_part_intervals() -> IntervalSet {
    IntervalSet::new(vec![
        (b'!' as u32, b'!' as u32),  // !
        (b'#' as u32, b'\'' as u32), // # $ % & '
        (b'*' as u32, b'+' as u32),  // * +
        (b'-' as u32, b'-' as u32),  // -
        (b'/' as u32, b'9' as u32),  // / 0-9
        (b'=' as u32, b'=' as u32),  // =
        (b'A' as u32, b'Z' as u32),  // A-Z
        (b'^' as u32, b'`' as u32),  // ^ _ `
        (b'a' as u32, b'z' as u32),  // a-z
        (b'{' as u32, b'~' as u32),  // { | } ~
    ])
}

/// `string.printable` from Python: ASCII 32..=126 plus the whitespace
/// codepoints `\t \n \x0b \x0c \r` (9..=13). Used for URL path components
/// before url-encoding.
fn printable_ascii_intervals() -> IntervalSet {
    IntervalSet::new(vec![(9, 13), (32, 126)])
}

/// Latin-1 byte range used as the alphabet for URL fragment characters.
/// Mirrors Hypothesis's `st.characters(min_codepoint=0, max_codepoint=255)`
/// for `_url_fragments_strategy`. Surrogates `[0xD800, 0xDFFF]` aren't in
/// range so the single interval is already surrogate-free.
fn fragment_byte_intervals() -> IntervalSet {
    IntervalSet::new(vec![(0, 0xFF)])
}

/// Encode a `String` as a CBOR tag-91 value, the wire format used by the
/// hegel server for strings (`HEGEL_STRING_TAG = 91` in `hegel.schema`).
fn encode_string(s: String) -> Value {
    Value::Tag(91, Box::new(Value::Bytes(s.into_bytes())))
}

fn mark_invalid(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    ntc.conclude(Status::Invalid, None);
    Err(EngineError::InvalidTestCase)
}

/// `domain` schema → an RFC 1035 fully-qualified domain name.
///
/// Port of Hypothesis's `DomainNameStrategy`:
///   1. Pick a TLD from the IANA list, filtered to `len(tld) + 2 <= max_length`.
///   2. Randomly flip the case of each TLD character.
///   3. Prepend 1..=126 dot-separated labels via `cu.many`, stopping early
///      if adding the next label would exceed `max_length`.
///
/// Each label matches the RFC 1035 regex
/// `[a-zA-Z]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?` and excludes labels that
/// start with `xn--` (RFC 5890 reserves these for punycode internationalised
/// domain names — Hypothesis filters them too).
pub(super) fn interpret_domain(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    let max_length = map_get(schema, "max_length")
        .and_then(as_u64)
        .unwrap_or(255) as usize;
    Ok(encode_string(draw_domain(ntc, max_length)?))
}

/// Shared domain-string draw used by `interpret_domain`, `interpret_email`
/// and `interpret_url`. `max_length` follows `DomainNameStrategy.max_length`
/// (RFC 1035 §2.3.4): 4..=255.
fn draw_domain(ntc: &mut NativeTestCase, max_length: usize) -> Result<String, EngineError> {
    // RFC 1035 §2.3.4 limits a single label to 63 octets. Hypothesis
    // exposes this as a parameter, but the `DomainGenerator` schema doesn't,
    // so we hard-code the RFC limit here.
    const MAX_LABEL_LEN: usize = 63;

    let eligible: Vec<&'static str> = TOP_LEVEL_DOMAINS
        .iter()
        .copied()
        .filter(|tld| tld.len() + 2 <= max_length)
        .collect();
    // `DomainGenerator::build_schema` rejects `max_length < 4` upstream and
    // the shortest TLD in the IANA list is 2 chars, so `eligible` is only
    // empty for a malformed schema from another client — surface that as
    // InvalidArgument like every other malformed-schema path here
    // (Hypothesis's provisional.domains() raises InvalidArgument too).
    if eligible.is_empty() {
        return Err(EngineError::InvalidArgument(format!(
            "domain max_length={max_length} leaves no eligible TLDs"
        )));
    }
    let idx = ntc
        .draw_integer(BigInt::from(0), BigInt::from(eligible.len() as i64 - 1))?
        .to_i128()
        .unwrap() as usize;
    let tld = eligible[idx];

    // Random recase: each char is flipped with p=0.5 (i.e. result is upper
    // or lower with equal probability). TLDs in the IANA list are
    // uppercase, so this matches Hypothesis's `_recase_randomly`.
    let mut domain = String::with_capacity(tld.len());
    for c in tld.chars() {
        let flip = ntc.weighted(0.5, None)?;
        domain.push(if flip { c.to_ascii_lowercase() } else { c });
    }

    // Prepend 1..=126 subdomain labels. The min_size=1 ensures the domain
    // always has at least one subdomain (so it always parses as
    // `<sub>.<tld>` — two dot-separated parts). The cap at 126 is from
    // Hypothesis: with 1-char labels at minimum, 126 * 2 = 252 chars plus
    // 3 for the TLD = 255, fitting the RFC 1035 §2.3.4 limit.
    let mut state = ManyState::new(1, Some(126));
    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let label = draw_dns_label(ntc, MAX_LABEL_LEN)?;
        if domain.len() + 1 + label.len() > max_length {
            // Adding this label would overflow `max_length`. Hypothesis
            // discards the just-opened span and breaks the loop; native
            // doesn't open a span per element here, so we just stop.
            break;
        }
        let mut next = String::with_capacity(label.len() + 1 + domain.len());
        next.push_str(&label);
        next.push('.');
        next.push_str(&domain);
        domain = next;
    }

    Ok(domain)
}

/// Draw one RFC 1035 / RFC 1123 label: starts with a letter, ends with
/// alphanumeric, middle is alphanumeric or hyphen, with the RFC 5890
/// reservation `label[2:4] != "--"` honoured inline.
///
/// Hypothesis enforces the RFC 5890 reservation as a filter-retry on the
/// regex strategy. Index 3 is the only middle position that can complete a
/// `--` window at indices `[2:4]`: the last position is always
/// alphanumeric, and position 1 is too close to the start. We inline that
/// constraint by coercing index 3 to alphanumeric whenever index 2 was
/// drawn as a hyphen, producing the same alphabet without a retry loop.
fn draw_dns_label(ntc: &mut NativeTestCase, max_len: usize) -> Result<String, EngineError> {
    let len = ntc
        .draw_integer(BigInt::from(1), BigInt::from(max_len as i64))?
        .to_i128()
        .unwrap() as usize;
    let mut s = String::with_capacity(len);
    s.push(draw_ascii_letter(ntc)?);
    if len > 1 {
        for i in 1..(len - 1) {
            let avoid_dash = i == 3 && s.as_bytes()[2] == b'-';
            s.push(if avoid_dash {
                draw_ascii_alnum(ntc)?
            } else {
                draw_ascii_alnum_or_hyphen(ntc)?
            });
        }
        s.push(draw_ascii_alnum(ntc)?);
    }
    Ok(s)
}

fn draw_ascii_letter(ntc: &mut NativeTestCase) -> Result<char, EngineError> {
    let i = ntc
        .draw_integer(BigInt::from(0), BigInt::from(51))?
        .to_i128()
        .unwrap() as u8;
    let b = if i < 26 { b'a' + i } else { b'A' + (i - 26) };
    Ok(b as char)
}

fn draw_ascii_alnum(ntc: &mut NativeTestCase) -> Result<char, EngineError> {
    let i = ntc
        .draw_integer(BigInt::from(0), BigInt::from(61))?
        .to_i128()
        .unwrap() as u8;
    let b = if i < 26 {
        b'a' + i
    } else if i < 52 {
        b'A' + (i - 26)
    } else {
        b'0' + (i - 52)
    };
    Ok(b as char)
}

fn draw_ascii_alnum_or_hyphen(ntc: &mut NativeTestCase) -> Result<char, EngineError> {
    let i = ntc
        .draw_integer(BigInt::from(0), BigInt::from(62))?
        .to_i128()
        .unwrap() as u8;
    let b = if i < 26 {
        b'a' + i
    } else if i < 52 {
        b'A' + (i - 26)
    } else if i < 62 {
        b'0' + (i - 52)
    } else {
        b'-'
    };
    Ok(b as char)
}

/// `email` schema → an RFC 5321/5322 email address like `alice@example.com`.
///
/// Port of Hypothesis's `emails()`:
///   - Local part: 1..=64 chars from the RFC 5322 atext set.
///   - Domain: as `draw_domain(255)`.
///   - Filter: overall length ≤ 254 (RFC 5321 §4.5.3.1.3). Implemented by
///     marking the test case invalid when the filter fails — the engine
///     then retries with a different choice prefix.
pub(super) fn interpret_email(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let local = ntc.draw_string(email_local_part_intervals(), 1, 64)?;
    let domain = draw_domain(ntc, 255)?;
    let address = format!("{local}@{domain}");
    if address.len() > 254 {
        return mark_invalid(ntc);
    }
    Ok(encode_string(address))
}

/// `url` schema → an RFC 3986 `http`/`https` URL.
///
/// Port of Hypothesis's `urls()` template
/// `"{scheme}://{domain}{port?}/{path}{fragment?}"`:
///   - `scheme` ∈ {http, https} (sampled_from).
///   - `domain` from `draw_domain(255)`.
///   - `port` is either empty or `:N` for `N ∈ 1..=65535`.
///   - `path` is the `/`-join of 0..N path components, each a url-encoded
///     `text(string.printable)` value.
///   - `fragment` is empty or `#…` of url-encoded chars in `0..=255`.
pub(super) fn interpret_url(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let scheme = if ntc.weighted(0.5, None)? {
        "https"
    } else {
        "http"
    };

    let domain = draw_domain(ntc, 255)?;

    // `st.just("") | ports` is a `one_of` over two alternatives — uniform
    // index selection. The port range matches `integers(1, 65535)`.
    let port = if ntc.weighted(0.5, None)? {
        format!(
            ":{}",
            ntc.draw_integer(BigInt::from(1), BigInt::from(65535))?
                .to_i128()
                .unwrap()
        )
    } else {
        String::new()
    };

    // `paths = lists(text(string.printable).map(url_encode)).map("/".join)`
    // — variable-length list of url-encoded printable strings, joined by
    // `/`. The outer `"{}://{}{}/{}{}".format` already supplies the leading
    // `/`, so a `paths_list == []` produces `…/`.
    let printable = printable_ascii_intervals();
    let mut state = ManyState::new(0, None);
    let mut components: Vec<String> = Vec::new();
    loop {
        if !many_more(ntc, &mut state)? {
            break;
        }
        let raw = ntc.draw_string(printable.clone(), 0, 100)?;
        components.push(url_encode_path(&raw));
    }
    let path = components.join("/");

    let fragment = if ntc.weighted(0.5, None)? {
        let raw = ntc.draw_string(fragment_byte_intervals(), 1, 100)?;
        format!("#{}", url_encode_fragment(ntc, &raw)?)
    } else {
        String::new()
    };

    let url = format!("{scheme}://{domain}{port}/{path}{fragment}");
    Ok(encode_string(url))
}

/// `URL_SAFE_CHARACTERS` from Hypothesis: `ascii_letters + digits +
/// "$-_.+!*'(),~"`. Characters outside this set are percent-encoded as
/// `%XX` where `XX` is the codepoint in uppercase hex.
fn is_url_safe(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '$' | '-' | '_' | '.' | '+' | '!' | '*' | '\'' | '(' | ')' | ',' | '~'
        )
}

/// `FRAGMENT_SAFE_CHARACTERS = URL_SAFE_CHARACTERS | {"?", "/"}`.
fn is_fragment_safe(c: char) -> bool {
    is_url_safe(c) || c == '?' || c == '/'
}

/// Apply Hypothesis's `url_encode` to a path component: each char is either
/// passed through (if URL-safe) or rendered as `%XX`.
fn url_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if is_url_safe(c) {
            out.push(c);
        } else {
            push_percent_encoded(&mut out, c);
        }
    }
    out
}

/// Apply Hypothesis's `_url_fragments_strategy` per-char rule: with
/// probability 0.5 force `%XX`, otherwise `%XX` only if the char isn't in
/// `FRAGMENT_SAFE_CHARACTERS`.
fn url_encode_fragment(ntc: &mut NativeTestCase, s: &str) -> Result<String, EngineError> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let force_encode = ntc.weighted(0.5, None)?;
        if force_encode || !is_fragment_safe(c) {
            push_percent_encoded(&mut out, c);
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// Emit `%XX` for a single Latin-1 char. Inputs are bounded to codepoints
/// 0..=255 by the calling alphabets (`printable_ascii_intervals` and
/// `fragment_byte_intervals`), so the cast to `u8` is always exact.
fn push_percent_encoded(out: &mut String, c: char) {
    let cp = c as u32;
    hegel_internal_debug_assert!(
        cp <= 0xFF,
        "push_percent_encoded called with codepoint > 0xFF: {cp:#x}"
    );
    out.push_str(&format!("%{:02X}", cp & 0xFF));
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/internet_tests.rs"]
mod tests;
