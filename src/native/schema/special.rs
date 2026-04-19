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

/// Draw a short hostname label: 3–8 lowercase letters.
fn draw_label(ntc: &mut NativeTestCase) -> Result<String, StopTest> {
    let len = ntc.draw_integer(3, 8)? as usize;
    draw_letters(ntc, len)
}

/// Draw a top-level domain: 2–4 lowercase letters.
fn draw_tld(ntc: &mut NativeTestCase) -> Result<String, StopTest> {
    let len = ntc.draw_integer(2, 4)? as usize;
    draw_letters(ntc, len)
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
pub(super) fn interpret_date(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let year = ntc.draw_integer(1970, 2100)?;
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
pub(super) fn interpret_datetime(ntc: &mut NativeTestCase) -> Result<Value, StopTest> {
    let year = ntc.draw_integer(1970, 2100)?;
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

/// `domain` schema → a hostname like `sub.example.com`, respecting `max_length`.
///
/// Structure: up to 2 subdomain labels + a second-level label + TLD, joined by dots.
/// Total length always ≤ max_length (capped conservatively).
pub(super) fn interpret_domain(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, StopTest> {
    use crate::cbor_utils::as_u64;
    let max_length = map_get(schema, "max_length")
        .and_then(as_u64)
        .unwrap_or(255) as usize;

    // Draw the number of subdomain labels (0, 1, or 2) plus the SLD + TLD.
    // Minimum domain is "aaa.aa" = 6 chars; with 1 sub it's "aaa.aaa.aa" = 10; etc.
    let max_subs = if max_length >= 10 { 2 } else { 0 };
    let n_subs = ntc.draw_integer(0, max_subs)?;

    let mut parts: Vec<String> = Vec::new();
    for _ in 0..n_subs {
        parts.push(draw_label(ntc)?);
    }
    parts.push(draw_label(ntc)?); // SLD
    parts.push(draw_tld(ntc)?); // TLD

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
