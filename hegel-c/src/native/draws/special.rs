use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;

use crate::control::hegel_internal_assert;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{EngineError, NativeTestCase};

use super::{LABEL_DATE, LABEL_DATETIME, LABEL_IP_ADDRESS, LABEL_TIME, LABEL_UUID, spanned};

/// A drawn Gregorian calendar date. `year` ∈ [1, 9999], `month` ∈ [1, 12],
/// `day` ∈ [1, days-in-month].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// A drawn time of day. `hour` ∈ [0, 23], `minute`/`second` ∈ [0, 59],
/// `microsecond` ∈ [0, 999_999].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub microsecond: u32,
}

/// A drawn naive datetime (a [`Date`] plus a [`Time`], no timezone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub date: Date,
    pub time: Time,
}

/// Days in the given month of the Gregorian `year`. Leap years follow the
/// usual rule: divisible by 4 unless divisible by 100 but not 400.
fn days_in_month(year: i32, month: u8) -> i64 {
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

fn draw_i64(ntc: &mut NativeTestCase, min: i64, max: i64) -> Result<i64, EngineError> {
    Ok(ntc
        .draw_integer(BigInt::from(min), BigInt::from(max))?
        .to_i64()
        .unwrap())
}

/// The unspanned date draw shared by [`generate_date`] and
/// [`generate_datetime`].
///
/// Hypothesis draws year ∈ [1, 9999] with shrink_towards=2000, month ∈ [1, 12],
/// then day ∈ [1, days_in_month(year, month)]. Native lacks a parameterised
/// `shrink_towards` on `draw_integer` (always 0), so we draw the year as an
/// offset from 2000 to get the same shrink target. The observable
/// distribution is identical because `biased_integer_sample` boosts the
/// endpoints (here ±offset bounds) at the same rate either way.
pub(crate) fn draw_date(ntc: &mut NativeTestCase) -> Result<Date, EngineError> {
    let year = (2000 + draw_i64(ntc, 1 - 2000, 9999 - 2000)?) as i32;
    let month = draw_i64(ntc, 1, 12)? as u8;
    let day = draw_i64(ntc, 1, days_in_month(year, month))? as u8;
    Ok(Date { year, month, day })
}

/// The unspanned time draw shared by [`generate_time`] and
/// [`generate_datetime`].
pub(crate) fn draw_time(ntc: &mut NativeTestCase) -> Result<Time, EngineError> {
    let hour = draw_i64(ntc, 0, 23)? as u8;
    let minute = draw_i64(ntc, 0, 59)? as u8;
    let second = draw_i64(ntc, 0, 59)? as u8;
    let microsecond = draw_i64(ntc, 0, 999_999)? as u32;
    Ok(Time {
        hour,
        minute,
        second,
        microsecond,
    })
}

/// Draw a [`Date`], wrapped in a span so the shrinker treats it as a unit.
pub fn generate_date(ntc: &mut NativeTestCase) -> Result<Date, EngineError> {
    spanned(ntc, LABEL_DATE, draw_date)
}

/// Draw a [`Time`], wrapped in a span.
pub fn generate_time(ntc: &mut NativeTestCase) -> Result<Time, EngineError> {
    spanned(ntc, LABEL_TIME, draw_time)
}

/// Draw a [`DateTime`], wrapped in a span.
pub fn generate_datetime(ntc: &mut NativeTestCase) -> Result<DateTime, EngineError> {
    spanned(ntc, LABEL_DATETIME, |ntc| {
        let date = draw_date(ntc)?;
        let time = draw_time(ntc)?;
        Ok(DateTime { date, time })
    })
}

/// The unspanned UUID draw shared by [`generate_uuid`] and the schema
/// interpreter. Returns the UUID's 16 big-endian bytes.
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
/// "uuids never produce nil" property carries over.
pub(crate) fn draw_uuid(
    ntc: &mut NativeTestCase,
    version: Option<u8>,
) -> Result<[u8; 16], EngineError> {
    if let Some(v) = version {
        if v > 15 {
            return Err(EngineError::InvalidArgument(format!(
                "uuid version must be a single hex nibble (0..=15), got {v}"
            )));
        }
    }
    let hi = ntc
        .draw_integer(BigInt::from(0u64), BigInt::from(u64::MAX))?
        .to_u64()
        .unwrap();
    let lo = ntc
        .draw_integer(BigInt::from(0u64), BigInt::from(u64::MAX))?
        .to_u64()
        .unwrap();
    let mut n: u128 = (u128::from(hi) << 64) | u128::from(lo);

    if let Some(v) = version {
        n &= !(0xc000u128 << 48);
        n |= 0x8000u128 << 48;
        n &= !(0xf000u128 << 64);
        n |= u128::from(v) << 76;
    } else if n == 0 {
        n = 1;
    }

    Ok(n.to_be_bytes())
}

/// Draw a UUID's 16 big-endian bytes, wrapped in a span. When `version` is
/// set, the RFC 4122 version and variant nibbles are forced accordingly.
pub fn generate_uuid(
    ntc: &mut NativeTestCase,
    version: Option<u8>,
) -> Result<[u8; 16], EngineError> {
    spanned(ntc, LABEL_UUID, |ntc| draw_uuid(ntc, version))
}

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
    hegel_internal_assert!(
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
    hegel_internal_assert!(
        (1..=128).contains(&prefix),
        "IPv6 prefix must be in 1..=128, got /{prefix}"
    );
    let mask = u128::MAX << (128 - prefix);
    let base = u128::from(addr) & mask;
    let size_minus_1 = !mask;
    hegel_internal_assert!(
        size_minus_1 <= i128::MAX as u128,
        "IPv6 special range too wide to fit offset in i128: prefix /{prefix}"
    );
    V6Network { base, size_minus_1 }
}

/// Draw an IPv4 address, wrapped in a span. Half the draws are uniform over
/// the whole address space and half are biased into the IANA
/// special-purpose ranges, mirroring `st.ip_addresses(v=4)`.
pub fn generate_ipv4(ntc: &mut NativeTestCase) -> Result<Ipv4Addr, EngineError> {
    spanned(ntc, LABEL_IP_ADDRESS, draw_ipv4)
}

/// Draw an IPv6 address, wrapped in a span, with the same special-range
/// biasing as [`generate_ipv4`].
pub fn generate_ipv6(ntc: &mut NativeTestCase) -> Result<Ipv6Addr, EngineError> {
    spanned(ntc, LABEL_IP_ADDRESS, draw_ipv6)
}

fn draw_ipv4(ntc: &mut NativeTestCase) -> Result<Ipv4Addr, EngineError> {
    let addr_int: u32 = if draw_i64(ntc, 0, 1)? == 0 {
        let a = draw_i64(ntc, 0, 255)? as u32;
        let b = draw_i64(ntc, 0, 255)? as u32;
        let c = draw_i64(ntc, 0, 255)? as u32;
        let d = draw_i64(ntc, 0, 255)? as u32;
        (a << 24) | (b << 16) | (c << 8) | d
    } else {
        let nets = &*SPECIAL_IPV4_NETWORKS;
        let idx = draw_i64(ntc, 0, nets.len() as i64 - 1)? as usize;
        let net = &nets[idx];
        let offset = draw_i64(ntc, 0, i64::from(net.size_minus_1))? as u32;
        net.base + offset
    };
    Ok(Ipv4Addr::from(addr_int))
}

fn draw_ipv6(ntc: &mut NativeTestCase) -> Result<Ipv6Addr, EngineError> {
    let addr_int: u128 = if draw_i64(ntc, 0, 1)? == 0 {
        let hi = ntc
            .draw_integer(BigInt::from(0u64), BigInt::from(u64::MAX))?
            .to_u64()
            .unwrap();
        let lo = ntc
            .draw_integer(BigInt::from(0u64), BigInt::from(u64::MAX))?
            .to_u64()
            .unwrap();
        (u128::from(hi) << 64) | u128::from(lo)
    } else {
        let nets = &*SPECIAL_IPV6_NETWORKS;
        let idx = draw_i64(ntc, 0, nets.len() as i64 - 1)? as usize;
        let net = &nets[idx];
        let offset = ntc
            .draw_integer(BigInt::from(0u128), BigInt::from(net.size_minus_1))?
            .to_u128()
            .unwrap();
        net.base + offset
    };
    Ok(Ipv6Addr::from(addr_int))
}

#[cfg(test)]
#[path = "../../../tests/embedded/native/draws/special_tests.rs"]
mod tests;
