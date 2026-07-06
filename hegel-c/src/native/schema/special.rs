use crate::cbor_utils::map_get;
use crate::native::core::{EngineError, NativeTestCase};
use crate::native::draws::special::{draw_date, draw_ip_address, draw_time, draw_uuid};
use ciborium::Value;

/// Encode a `String` as a CBOR tag-91 value, the wire format used by the hegel
/// server for strings (`HEGEL_STRING_TAG = 91` in `hegel.schema`).
fn encode_string(s: String) -> Value {
    Value::Tag(91, Box::new(Value::Bytes(s.into_bytes())))
}

/// `date` schema → `YYYY-MM-DD`, matching `st.dates().isoformat()`.
pub(super) fn interpret_date(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let d = draw_date(ntc)?;
    Ok(encode_string(format!(
        "{:04}-{:02}-{:02}",
        d.year, d.month, d.day
    )))
}

/// `time` schema → `HH:MM:SS` or `HH:MM:SS.ffffff`, matching
/// `st.times().isoformat()`. The fractional part is present iff
/// `microsecond != 0` (Python's `time.isoformat()` semantics).
pub(super) fn interpret_time(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let t = draw_time(ntc)?;
    Ok(encode_string(format_time(
        t.hour,
        t.minute,
        t.second,
        t.microsecond,
    )))
}

/// `datetime` schema → `YYYY-MM-DDTHH:MM:SS[.ffffff]`, matching
/// `st.datetimes().isoformat()`. As with `interpret_time`, the fractional
/// seconds appear only when non-zero.
pub(super) fn interpret_datetime(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let d = draw_date(ntc)?;
    let t = draw_time(ntc)?;
    let time_part = format_time(t.hour, t.minute, t.second, t.microsecond);
    Ok(encode_string(format!(
        "{:04}-{:02}-{:02}T{time_part}",
        d.year, d.month, d.day
    )))
}

fn format_time(hour: u8, minute: u8, second: u8, microsecond: u32) -> String {
    if microsecond == 0 {
        format!("{hour:02}:{minute:02}:{second:02}")
    } else {
        format!("{hour:02}:{minute:02}:{second:02}.{microsecond:06}")
    }
}

/// `uuid` schema → canonical hyphenated UUID string, matching
/// `str(st.uuids(version=...))`.
pub(super) fn interpret_uuid(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    use crate::cbor_utils::as_u64;
    let version = map_get(schema, "version").and_then(as_u64).map(|v| v as u8);
    let bytes = draw_uuid(ntc, version)?;
    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(encode_string(format!(
        "{}{}{}{}-{}{}-{}{}-{}{}-{}{}{}{}{}{}",
        hex[0],
        hex[1],
        hex[2],
        hex[3],
        hex[4],
        hex[5],
        hex[6],
        hex[7],
        hex[8],
        hex[9],
        hex[10],
        hex[11],
        hex[12],
        hex[13],
        hex[14],
        hex[15],
    )))
}

/// `ip_address` schema → IPv4 dotted-decimal or IPv6 colon-hex string,
/// matching `str(st.ip_addresses(v=schema["version"]))`.
pub(super) fn interpret_ip_address(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    use crate::cbor_utils::as_u64;
    let version = map_get(schema, "version").and_then(as_u64).ok_or_else(|| {
        EngineError::InvalidArgument(
            "ip_address schema is missing an integer \"version\" field".to_string(),
        )
    })?;
    let version = u8::try_from(version).unwrap_or(u8::MAX);
    let addr = draw_ip_address(ntc, version)?;
    Ok(encode_string(addr.to_string()))
}
