// Interpreters for special string schemas: date, time, datetime, ip_address,
// uuid. All produce ciborium::Value strings matching the output of
// Hypothesis's `st.dates() / st.times() / st.datetimes() / st.ip_addresses() /
// st.uuids()` strategies (with `.isoformat()` / `str(...)` mapping applied
// server-side, per `hegel.schema._from_schema`).

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;

use crate::cbor_utils::map_get;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{EngineError, NativeTestCase};
use ciborium::Value;

/// Encode a `String` as a CBOR tag-91 value, the Hegel wire format for strings
/// (`HEGEL_STRING_TAG = 91` in `hegel.schema`).
fn encode_string(s: String) -> Value {
    Value::Tag(91, Box::new(Value::Bytes(s.into_bytes())))
}

/// Days in the given month of the Gregorian `year`. Leap years follow the
/// usual rule: divisible by 4 unless divisible by 100 but not 400.
fn days_in_month(year: i128, month: i128) -> i128 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            if is_leap { 29 } else { 28 }
        }
        _ => unreachable!("days_in_month: month {} out of range 1..=12", month),
    }
}

/// `date` schema → `YYYY-MM-DD`, matching `st.dates().isoformat()`.
///
/// Hypothesis draws year ∈ [1, 9999] with shrink_towards=2000, month ∈ [1, 12],
/// then day ∈ [1, days_in_month(year, month)]. Native lacks a parameterised
/// `shrink_towards` on `draw_integer` (always 0), so we draw the year as an
/// offset from 2000 to get the same shrink target. The observable
/// distribution is identical because `biased_integer_sample` boosts the
/// endpoints (here ±offset bounds) at the same rate either way.
pub(super) fn interpret_date(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let (year, month, day) = draw_date(ntc)?;
    Ok(encode_string(format!("{year:04}-{month:02}-{day:02}")))
}

/// `time` schema → `HH:MM:SS` or `HH:MM:SS.ffffff`, matching
/// `st.times().isoformat()`. The fractional part is present iff
/// `microsecond != 0` (Python's `time.isoformat()` semantics).
pub(super) fn interpret_time(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let (hour, minute, second, microsecond) = draw_time(ntc)?;
    Ok(encode_string(format_time(
        hour,
        minute,
        second,
        microsecond,
    )))
}

/// `datetime` schema → `YYYY-MM-DDTHH:MM:SS[.ffffff]`, matching
/// `st.datetimes().isoformat()`. As with `interpret_time`, the fractional
/// seconds appear only when non-zero.
pub(super) fn interpret_datetime(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let (year, month, day) = draw_date(ntc)?;
    let (hour, minute, second, microsecond) = draw_time(ntc)?;
    let time_part = format_time(hour, minute, second, microsecond);
    Ok(encode_string(format!(
        "{year:04}-{month:02}-{day:02}T{time_part}"
    )))
}

fn draw_date(ntc: &mut NativeTestCase) -> Result<(i128, i128, i128), EngineError> {
    let year = 2000
        + ntc
            .draw_integer(BigInt::from(1 - 2000), BigInt::from(9999 - 2000))?
            .to_i128()
            .unwrap();
    let month = ntc
        .draw_integer(BigInt::from(1), BigInt::from(12))?
        .to_i128()
        .unwrap();
    let day = ntc
        .draw_integer(BigInt::from(1), BigInt::from(days_in_month(year, month)))?
        .to_i128()
        .unwrap();
    Ok((year, month, day))
}

fn draw_time(ntc: &mut NativeTestCase) -> Result<(i128, i128, i128, i128), EngineError> {
    let hour = ntc
        .draw_integer(BigInt::from(0), BigInt::from(23))?
        .to_i128()
        .unwrap();
    let minute = ntc
        .draw_integer(BigInt::from(0), BigInt::from(59))?
        .to_i128()
        .unwrap();
    let second = ntc
        .draw_integer(BigInt::from(0), BigInt::from(59))?
        .to_i128()
        .unwrap();
    let microsecond = ntc
        .draw_integer(BigInt::from(0), BigInt::from(999_999))?
        .to_i128()
        .unwrap();
    Ok((hour, minute, second, microsecond))
}

fn format_time(hour: i128, minute: i128, second: i128, microsecond: i128) -> String {
    if microsecond == 0 {
        format!("{hour:02}:{minute:02}:{second:02}")
    } else {
        format!("{hour:02}:{minute:02}:{second:02}.{microsecond:06}")
    }
}

/// `uuid` schema → canonical hyphenated UUID string, matching
/// `str(st.uuids(version=...))`.
///
/// Hypothesis's `uuids()` strategy uses `random.getrandbits(128)` via
/// `use_true_random=True`, so its UUIDs sit outside the choice tree and do
/// not shrink. The native port records two 64-bit integer draws instead —
/// the observable distribution still matches (uniform over 2^128 values when
/// `version` is unset, with the RFC 4122 version + variant nibbles overridden
/// when `version` is set), but native UUIDs nominally shrink toward
/// all-zeros.
///
/// When `version` is unset and the draws land at all-zeros (which the
/// Generate-phase all-simplest pre-trial hits deterministically), the result
/// would be the nil UUID. Hypothesis can't produce nil because `getrandbits`
/// sits outside the choice tree; here we bump the low bit so the
/// "uuids never produce nil" property carries over. The proper fix is a
/// non-recording RNG-direct draw API on `NativeTestCase` to mirror Hypothesis
/// exactly — out of scope for this PR.
pub(super) fn interpret_uuid(
    ntc: &mut NativeTestCase,
    schema: &Value,
) -> Result<Value, EngineError> {
    use crate::cbor_utils::as_u64;
    let version = map_get(schema, "version").and_then(as_u64).map(|v| v as u8);

    // Two 64-bit halves: u64::MAX fits comfortably in i128 so the draw is in range.
    let hi = ntc
        .draw_integer(BigInt::from(0), BigInt::from(u64::MAX))?
        .to_u64()
        .unwrap();
    let lo = ntc
        .draw_integer(BigInt::from(0), BigInt::from(u64::MAX))?
        .to_u64()
        .unwrap();
    let mut n: u128 = (u128::from(hi) << 64) | u128::from(lo);

    if let Some(v) = version {
        // Clear and set the RFC 4122 variant (top 2 bits of the 9th byte = bits 62..63 of a
        // big-endian u128 = bits 48..63 within the second-from-low 16-bit group).
        // Hypothesis's `UUID(version=v, int=n)`: `n &= ~(0xc000 << 48); n |= 0x8000 << 48`.
        n &= !(0xc000u128 << 48);
        n |= 0x8000u128 << 48;
        // Clear and set the version nibble (high nibble of the 7th byte = bits 76..79).
        // `n &= ~(0xf000 << 64); n |= version << 76`.
        n &= !(0xf000u128 << 64);
        n |= u128::from(v) << 76;
    } else if n == 0 {
        n = 1;
    }

    let bytes = n.to_be_bytes();
    Ok(encode_string(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )))
}

// ── IP addresses ─────────────────────────────────────────────────────────────
//
// Hypothesis biases toward reserved / private / loopback / documentation
// ranges so bugs around special-address handling get exercised. Per
// `hypothesis.strategies._internal.ipaddress`:
//
//     four = binary(4).map(IPv4Address) | sampled_from(SPECIAL_IPv4_RANGES).flatmap(ip_in_network)
//     six  = binary(16).map(IPv6Address) | sampled_from(SPECIAL_IPv6_RANGES).flatmap(ip_in_network)
//
// `a | b` is `one_of([a, b])`, which draws an index in {0, 1} uniformly.
// `ip_in_network` draws an integer in `[int(network[0]), int(network[-1])]`
// uniformly.

// IANA's IPv4 special-purpose registry. Sourced from
// `hypothesis.strategies._internal.ipaddress::SPECIAL_IPv4_RANGES`.
const SPECIAL_IPV4_CIDRS: &[&str] = &[
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.0.0.0/24",
    "192.0.0.0/29",
    "192.0.0.8/32",
    "192.0.0.9/32",
    "192.0.0.10/32",
    "192.0.0.170/32",
    "192.0.0.171/32",
    "192.0.2.0/24",
    "192.31.196.0/24",
    "192.52.193.0/24",
    "192.88.99.0/24",
    "192.168.0.0/16",
    "192.175.48.0/24",
    "198.18.0.0/15",
    "198.51.100.0/24",
    "203.0.113.0/24",
    "240.0.0.0/4",
    "255.255.255.255/32",
];

// IANA's IPv6 special-purpose registry. Sourced from
// `hypothesis.strategies._internal.ipaddress::SPECIAL_IPv6_RANGES`.
const SPECIAL_IPV6_CIDRS: &[&str] = &[
    "::1/128",
    "::/128",
    "::ffff:0:0/96",
    "64:ff9b::/96",
    "64:ff9b:1::/48",
    "100::/64",
    "2001::/23",
    "2001::/32",
    "2001:1::1/128",
    "2001:1::2/128",
    "2001:2::/48",
    "2001:3::/32",
    "2001:4:112::/48",
    "2001:10::/28",
    "2001:20::/28",
    "2001:db8::/32",
    "2002::/16",
    "2620:4f:8000::/48",
    "fc00::/7",
    "fe80::/10",
];

/// A CIDR network as `(base address, size - 1)`. `size - 1` is the inclusive
/// upper bound on the host-part offset, drawn as `draw_integer(0, size_minus_1)`
/// then added to `base`. Storing the offset bound rather than the last address
/// avoids `as i128` reinterpret-casts on values above `i128::MAX` (e.g.
/// `fc00::/7` whose base has the top bit set).
struct V4Network {
    base: u32,
    size_minus_1: u32,
}

struct V6Network {
    base: u128,
    size_minus_1: u128,
}

static SPECIAL_IPV4_NETWORKS: LazyLock<Vec<V4Network>> = LazyLock::new(|| {
    SPECIAL_IPV4_CIDRS
        .iter()
        .map(|s| parse_v4_cidr(s))
        .collect()
});

static SPECIAL_IPV6_NETWORKS: LazyLock<Vec<V6Network>> = LazyLock::new(|| {
    SPECIAL_IPV6_CIDRS
        .iter()
        .map(|s| parse_v6_cidr(s))
        .collect()
});

fn parse_v4_cidr(s: &str) -> V4Network {
    let (addr, prefix) = s.split_once('/').unwrap();
    let addr: Ipv4Addr = addr.parse().unwrap();
    let prefix: u32 = prefix.parse().unwrap();
    // Every range we ship has prefix in 1..=32. /0 would shift by 32 and
    // overflow `u32`; reject it explicitly rather than silently masking it.
    assert!(
        (1..=32).contains(&prefix),
        "IPv4 prefix must be in 1..=32, got /{prefix}"
    );
    let mask = u32::MAX << (32 - prefix);
    let base = u32::from(addr) & mask;
    let size_minus_1 = !mask;
    V4Network { base, size_minus_1 }
}

fn parse_v6_cidr(s: &str) -> V6Network {
    let (addr, prefix) = s.split_once('/').unwrap();
    let addr: Ipv6Addr = addr.parse().unwrap();
    let prefix: u32 = prefix.parse().unwrap();
    // Every range we ship has prefix in 7..=128, comfortably under
    // i128::MAX (≤ 2^121 host bits). /0 would shift by 128 and overflow
    // `u128`; reject it explicitly.
    assert!(
        (1..=128).contains(&prefix),
        "IPv6 prefix must be in 1..=128, got /{prefix}"
    );
    let mask = u128::MAX << (128 - prefix);
    let base = u128::from(addr) & mask;
    let size_minus_1 = !mask;
    assert!(
        size_minus_1 <= i128::MAX as u128,
        "IPv6 special range too wide to fit offset in i128: prefix /{prefix}"
    );
    V6Network { base, size_minus_1 }
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
    match version {
        4 => interpret_ipv4(ntc),
        6 => interpret_ipv6(ntc),
        other => Err(EngineError::InvalidArgument(format!(
            "ip_address: unsupported version {other}; expected 4 or 6"
        ))),
    }
}

fn interpret_ipv4(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    // one_of([random_bytes, sampled_from(SPECIAL).flatmap(in_network)]).
    let addr_int: u32 = if ntc
        .draw_integer(BigInt::from(0), BigInt::from(1))?
        .to_i128()
        .unwrap()
        == 0
    {
        // Four uniform bytes — `binary(min_size=4, max_size=4).map(IPv4Address)`.
        let a = ntc
            .draw_integer(BigInt::from(0), BigInt::from(255))?
            .to_u32()
            .unwrap();
        let b = ntc
            .draw_integer(BigInt::from(0), BigInt::from(255))?
            .to_u32()
            .unwrap();
        let c = ntc
            .draw_integer(BigInt::from(0), BigInt::from(255))?
            .to_u32()
            .unwrap();
        let d = ntc
            .draw_integer(BigInt::from(0), BigInt::from(255))?
            .to_u32()
            .unwrap();
        (a << 24) | (b << 16) | (c << 8) | d
    } else {
        let nets = &*SPECIAL_IPV4_NETWORKS;
        let idx = ntc
            .draw_integer(BigInt::from(0), BigInt::from(nets.len() as i64 - 1))?
            .to_i128()
            .unwrap() as usize;
        let net = &nets[idx];
        // integers(int(network[0]), int(network[-1])) — drawn as
        // base + offset so the i128 cast never sees a value above i128::MAX.
        let offset = ntc
            .draw_integer(BigInt::from(0), BigInt::from(net.size_minus_1))?
            .to_u32()
            .unwrap();
        net.base + offset
    };
    Ok(encode_string(Ipv4Addr::from(addr_int).to_string()))
}

fn interpret_ipv6(ntc: &mut NativeTestCase) -> Result<Value, EngineError> {
    let addr_int: u128 = if ntc
        .draw_integer(BigInt::from(0), BigInt::from(1))?
        .to_i128()
        .unwrap()
        == 0
    {
        // 16 uniform bytes via two 64-bit halves.
        let hi = ntc
            .draw_integer(BigInt::from(0), BigInt::from(u64::MAX))?
            .to_u64()
            .unwrap();
        let lo = ntc
            .draw_integer(BigInt::from(0), BigInt::from(u64::MAX))?
            .to_u64()
            .unwrap();
        (u128::from(hi) << 64) | u128::from(lo)
    } else {
        let nets = &*SPECIAL_IPV6_NETWORKS;
        let idx = ntc
            .draw_integer(BigInt::from(0), BigInt::from(nets.len() as i64 - 1))?
            .to_i128()
            .unwrap() as usize;
        let net = &nets[idx];
        let offset = ntc
            .draw_integer(BigInt::from(0), BigInt::from(net.size_minus_1))?
            .to_u128()
            .unwrap();
        net.base + offset
    };
    Ok(encode_string(Ipv6Addr::from(addr_int).to_string()))
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/schema/special_tests.rs"]
mod tests;
