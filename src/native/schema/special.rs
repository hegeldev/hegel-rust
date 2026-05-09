// Interpreters for special string schemas: date, time, datetime, ipv4, ipv6,
// domain, email, url.
//
// All produce ciborium::Value::Text strings.  Generation uses draw_integer and
// weighted draws so the shrinker can reduce them.

use crate::cbor_utils::map_get;
use crate::native::core::{NativeTestCase, StopTest};
use ciborium::Value;

/// Draw `len` lowercase ASCII letters (a-z) and return them as a String.
fn draw_letters(ntc: &mut NativeTestCase, len: usize) -> Result<String, StopTest> {
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        let c = ntc.draw_integer(0, 25)? as u8 + b'a';
        s.push(c as char);
    }
    Ok(s)
}

/// Draw a short hostname-style letter-only label: 3–8 lowercase letters.
/// Used by `interpret_email` / `interpret_url` whose hostnames don't go
/// through `interpret_domain`. Switching these to the RFC-compliant
/// `draw_dns_label` is a separate item — see A3 follow-ups.
fn draw_label(ntc: &mut NativeTestCase) -> Result<String, StopTest> {
    let len = ntc.draw_integer(3, 8)? as usize;
    draw_letters(ntc, len)
}

/// Draw a top-level domain: 2–4 lowercase letters. Same caveat as
/// [`draw_label`].
fn draw_tld(ntc: &mut NativeTestCase) -> Result<String, StopTest> {
    let len = ntc.draw_integer(2, 4)? as usize;
    draw_letters(ntc, len)
}

/// Draw an RFC 1035-compliant domain label of the given length.
///
/// Per RFC 1035 §2.3.1 (relaxed by RFC 1123 §2.1):
///   - First character: ASCII letter.
///   - Last character (if `len > 1`): ASCII letter or digit.
///   - Middle characters: ASCII letters, digits, or hyphens.
///
/// Letters dominate the boundary positions to keep generated names
/// predominantly readable; the middle positions span the full
/// letter/digit/hyphen alphabet so generators surface bugs in code
/// paths that handle alphanumeric and hyphenated labels — very common
/// in real DNS.
fn draw_dns_label(ntc: &mut NativeTestCase, len: usize) -> Result<String, StopTest> {
    let mut s = String::with_capacity(len);
    // First char: letter.
    s.push((ntc.draw_integer(0, 25)? as u8 + b'a') as char);
    if len == 1 {
        return Ok(s);
    }
    // Middle chars: letter / digit / hyphen.
    for _ in 1..(len - 1) {
        let idx = ntc.draw_integer(0, 36)? as u8;
        let c = match idx {
            0..=25 => idx + b'a',
            26..=35 => idx - 26 + b'0',
            _ => b'-',
        };
        s.push(c as char);
    }
    // Last char: letter or digit (no trailing hyphen).
    let idx = ntc.draw_integer(0, 35)? as u8;
    let c = if idx < 26 { idx + b'a' } else { idx - 26 + b'0' };
    s.push(c as char);
    Ok(s)
}

/// Encode a `String` as a CBOR tag-91 value (the wire format used by the hegel
/// server for strings).  All string-producing native schema interpreters must
/// use this helper so that `deserialize_value` can decode the result correctly.
fn encode_string(s: String) -> Value {
    Value::Tag(91, Box::new(Value::Bytes(s.into_bytes())))
}

/// `date` schema → YYYY-MM-DD.
///
/// Year in [1970, 2100], month in [1, 12], day in [1, 28] (28 is valid for all months).
///
/// The year is drawn as `2000 + offset` so that shrinking pulls offset toward
/// zero — yielding 2000-01-01 as the minimal date. This matches Hypothesis's
/// `dates()` strategy, which also anchors on the millennium rather than the
/// generator's lower bound.
pub(super) fn interpret_date(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let year_offset = ntc.draw_integer(1970 - 2000, 2100 - 2000)?;
    let year = 2000 + year_offset;
    let month = ntc.draw_integer(1, 12)?;
    let day = ntc.draw_integer(1, 28)?;
    Ok(encode_string(format!("{year:04}-{month:02}-{day:02}")))
}

/// `time` schema → HH:MM:SS.
///
/// Encodes as total seconds in [0, 86399] rather than three independent draws,
/// so that midnight (0) is a single boundary case that the engine finds reliably.
/// Drawing hour/minute/second independently would require all three to hit 0
/// simultaneously, which is extremely unlikely (~0.003% per test case).
pub(super) fn interpret_time(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let total_secs = ntc.draw_integer(0, 86399)?;
    let hour = total_secs / 3600;
    let minute = (total_secs % 3600) / 60;
    let second = total_secs % 60;
    Ok(encode_string(format!("{hour:02}:{minute:02}:{second:02}")))
}

/// `datetime` schema → YYYY-MM-DDTHH:MM:SS.
///
/// Year is anchored at 2000 (see `interpret_date` for rationale).
pub(super) fn interpret_datetime(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let year_offset = ntc.draw_integer(1970 - 2000, 2100 - 2000)?;
    let year = 2000 + year_offset;
    let month = ntc.draw_integer(1, 12)?;
    let day = ntc.draw_integer(1, 28)?;
    let hour = ntc.draw_integer(0, 23)?;
    let minute = ntc.draw_integer(0, 59)?;
    let second = ntc.draw_integer(0, 59)?;
    Ok(encode_string(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    )))
}

/// `ipv4` schema → dotted-decimal string like `192.168.1.1`.
pub(super) fn interpret_ipv4(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let a = ntc.draw_integer(0, 255)?;
    let b = ntc.draw_integer(0, 255)?;
    let c = ntc.draw_integer(0, 255)?;
    let d = ntc.draw_integer(0, 255)?;
    Ok(encode_string(format!("{a}.{b}.{c}.{d}")))
}

/// `ipv6` schema → full 8-group colon-hex string like `2001:0db8:0000:0000:0000:0000:0000:0001`.
pub(super) fn interpret_ipv6(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let mut groups = [0i128; 8];
    for g in &mut groups {
        *g = ntc.draw_integer(0, 0xFFFF)?;
    }
    let s = groups
        .iter()
        .map(|g| format!("{g:04x}"))
        .collect::<Vec<_>>()
        .join(":");
    Ok(encode_string(s))
}

/// `ip_address` schema with `version` field → delegates to `interpret_ipv4` or `interpret_ipv6`.
pub(super) fn interpret_ip_address(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    use crate::cbor_utils::as_u64;
    match map_get(schema, "version").and_then(as_u64) {
        Some(4) => interpret_ipv4(ntc),
        _ => interpret_ipv6(ntc),
    }
}

/// `domain` schema → a hostname like `sub.example.com`, respecting `max_length`.
///
/// Structure: a TLD + an SLD, optionally preceded by up to two subdomain
/// labels, all joined by dots. Each label is RFC 1035 / RFC 1123 compliant
/// (letters, digits, hyphens; letter-start; letter-or-digit-end).
///
/// Lengths are budgeted from right to left (TLD, SLD, then subs) so the
/// total length is *guaranteed* never to exceed `max_length`. The minimum
/// is `"a.aa"` (4 chars), which is the smallest valid generator setting.
pub(super) fn interpret_domain(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    use crate::cbor_utils::as_u64;
    let max_length = map_get(schema, "max_length")
        .and_then(as_u64)
        .unwrap_or(255) as i128;

    // TLD: 2..=4 letters, capped by remaining budget. Minimum domain is
    // "a.aa" (4 chars) so max_length ≥ 4 is enforced upstream by
    // `DomainGenerator::build_schema`. Reserving 2 chars (1 SLD + 1 dot)
    // leaves at most `max_length - 2` for the TLD.
    let tld_len_max = i128::min(4, max_length - 2);
    let tld_len = ntc.draw_integer(2, tld_len_max)? as usize;
    let tld = draw_letters(ntc, tld_len)?;
    let mut remaining = max_length - tld_len as i128 - 1; // 1 for the dot before TLD

    // SLD: 1..=8 chars, capped by remaining budget (must leave ≥0 for subs).
    let sld_len_max = i128::min(8, remaining);
    let sld_len = ntc.draw_integer(1, sld_len_max)? as usize;
    let sld = draw_dns_label(ntc, sld_len)?;
    remaining -= sld_len as i128;

    // Subdomains: 0..=2 of them. Each adds at least 2 chars ("a." prefix),
    // so n_subs is bounded by floor(remaining / 2). We also cap at 2 to
    // match the pre-A3 structural shape (most realistic domains have 0–2
    // subdomain levels).
    let max_subs_by_budget = if remaining >= 2 { remaining / 2 } else { 0 };
    let n_subs = ntc.draw_integer(0, i128::min(2, max_subs_by_budget))?;
    let mut subs: Vec<String> = Vec::with_capacity(n_subs as usize);
    for _ in 0..n_subs {
        // Each sub costs label_len + 1 (dot). Need ≥ 2 chars remaining
        // (1-char label + dot) to fit at all.
        if remaining < 2 {
            break;
        }
        let label_len_max = i128::min(8, remaining - 1); // -1 for the dot
        let label_len = ntc.draw_integer(1, label_len_max)? as usize;
        subs.push(draw_dns_label(ntc, label_len)?);
        remaining -= label_len as i128 + 1;
    }

    // Assemble: subs (in draw order) ++ [SLD, TLD], joined by '.'.
    let mut parts: Vec<String> = subs;
    parts.push(sld);
    parts.push(tld);
    Ok(encode_string(parts.join(".")))
}

/// `email` schema → a simple valid-looking email address like `alice@example.com`.
pub(super) fn interpret_email(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    // Username: 3–15 lowercase letters.
    let user_len = ntc.draw_integer(3, 15)? as usize;
    let user = draw_letters(ntc, user_len)?;

    // Domain: label.tld
    let domain = draw_label(ntc)?;
    let tld = draw_tld(ntc)?;

    Ok(encode_string(format!("{user}@{domain}.{tld}")))
}

/// `url` schema → a simple HTTP/HTTPS URL like `https://example.com/path`.
pub(super) fn interpret_url(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    // Scheme: http or https.
    let use_https = ntc.weighted(0.5, None)?;
    let scheme = if use_https { "https" } else { "http" };

    // Host: domain label + TLD.
    let host_label = draw_label(ntc)?;
    let tld = draw_tld(ntc)?;
    let host = format!("{host_label}.{tld}");

    // Path: 0–3 path components.
    let n_components = ntc.draw_integer(0, 3)?;
    let mut path = String::new();
    for _ in 0..n_components {
        let component_len = ntc.draw_integer(2, 8)? as usize;
        let component = draw_letters(ntc, component_len)?;
        path.push('/');
        path.push_str(&component);
    }

    Ok(encode_string(format!("{scheme}://{host}{path}")))
}

/// `uuid` schema → canonical hyphenated UUID string `xxxxxxxx-xxxx-Mxxx-Nxxx-xxxxxxxxxxxx`.
///
/// When `version` is specified, the version nibble (M) is set accordingly. When
/// unspecified, a version is drawn uniformly from `{1..=5}` so the generator
/// matches its documented "any version" default (`UuidsGenerator` doc in
/// `src/generators/strings.rs`). RFC 4122 variant bits (N ∈ {8,9,a,b}) are
/// always applied. The nil UUID is never produced.
pub(super) fn interpret_uuid(ntc: &mut NativeTestCase, schema: &Value) -> Result<Value, StopTest> {
    let version: i128 = match map_get(schema, "version").and_then(crate::cbor_utils::as_u64) {
        Some(v) => v as i128,
        // Schema-side `gs::uuids()` (no `.version(...)`) emits no
        // `version` field. Pick uniformly across the RFC 4122 versions.
        None => ntc.draw_integer(1, 5)?,
    };

    let g1 = ntc.draw_integer(0, 0xFFFF_FFFF)?; // 32 bits: time_low
    let g2 = ntc.draw_integer(0, 0xFFFF)?; // 16 bits: time_mid
    let g3_low = ntc.draw_integer(0, 0x0FFF)?; // 12 bits: time_high
    let g4_low = ntc.draw_integer(0, 0x3FFF)?; // 14 bits: clock_seq
    let g5_hi = ntc.draw_integer(0, 0xFFFF)?; // 16 bits: node high
    let g5_lo = ntc.draw_integer(0, 0xFFFF_FFFF)?; // 32 bits: node low

    let g3 = g3_low | (version << 12); // version in top nibble of third group
    let g4 = g4_low | 0x8000; // RFC 4122 variant: top 2 bits = 10

    Ok(encode_string(format!(
        "{g1:08x}-{g2:04x}-{g3:04x}-{g4:04x}-{g5_hi:04x}{g5_lo:08x}"
    )))
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/special_tests.rs"]
mod tests;
