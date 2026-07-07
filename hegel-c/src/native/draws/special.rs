use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;

use crate::control::hegel_internal_assert;
use crate::native::bignum::{BigInt, ToPrimitive};
use crate::native::core::{EngineError, NativeTestCase};

use super::{LABEL_DATE, LABEL_DATETIME, LABEL_IP_ADDRESS, LABEL_TIME, LABEL_UUID, spanned};

/// A drawn proleptic Gregorian calendar date. `year` ∈
/// [[`MIN_YEAR`], [`MAX_YEAR`]], `month` ∈ [1, 12], `day` ∈
/// [1, days-in-month].
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

/// Inclusive year envelope accepted by the bounded date draws. Wide enough
/// for every date type the frontends expose (chrono's `NaiveDate` spans
/// years ±262143) while keeping the day arithmetic comfortably inside `i64`.
pub const MIN_YEAR: i32 = -999_999;
pub const MAX_YEAR: i32 = 999_999;

/// Days from 1970-01-01 to `d` in the proleptic Gregorian calendar. Port of
/// Howard Hinnant's `days_from_civil`.
pub(crate) fn days_from_civil(d: &Date) -> i64 {
    let y = i64::from(d.year) - i64::from(d.month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from(d.month) + if d.month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(d.day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`]. Port of Hinnant's `civil_from_days`.
pub(crate) fn civil_from_days(z: i64) -> Date {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    Date {
        year: (y + i64::from(month <= 2)) as i32,
        month,
        day,
    }
}

fn validate_date(what: &str, d: &Date) -> Result<(), EngineError> {
    let valid = (MIN_YEAR..=MAX_YEAR).contains(&d.year)
        && (1..=12).contains(&d.month)
        && d.day >= 1
        && i64::from(d.day) <= days_in_month(d.year, d.month);
    if !valid {
        return Err(EngineError::InvalidArgument(format!(
            "{what} is not a valid date (year in [{MIN_YEAR}, {MAX_YEAR}]): \
             {:05}-{:02}-{:02}",
            d.year, d.month, d.day
        )));
    }
    Ok(())
}

fn validate_time(what: &str, t: &Time) -> Result<(), EngineError> {
    let valid = t.hour <= 23 && t.minute <= 59 && t.second <= 59 && t.microsecond <= 999_999;
    if !valid {
        return Err(EngineError::InvalidArgument(format!(
            "{what} is not a valid time: {:02}:{:02}:{:02}.{:06}",
            t.hour, t.minute, t.second, t.microsecond
        )));
    }
    Ok(())
}

pub(crate) fn time_to_us(t: &Time) -> i64 {
    ((i64::from(t.hour) * 60 + i64::from(t.minute)) * 60 + i64::from(t.second)) * 1_000_000
        + i64::from(t.microsecond)
}

fn time_from_us(us: i64) -> Time {
    Time {
        hour: (us / 3_600_000_000) as u8,
        minute: (us / 60_000_000 % 60) as u8,
        second: (us / 1_000_000 % 60) as u8,
        microsecond: (us % 1_000_000) as u32,
    }
}

const LAST_US_OF_DAY: i64 = 86_400_000_000 - 1;

/// The unspanned bounded date draw shared by [`generate_date`] and
/// [`generate_datetime`].
///
/// Mirrors Hypothesis's `DateStrategy`: one integer draw of a day offset
/// within the bounds, centred on 2000-01-01 (clamped into range) so that is
/// the shrink target. Native lacks a parameterised `shrink_towards` on
/// `draw_integer` (always 0), so the offset is drawn relative to the centre.
fn draw_date_in(
    ntc: &mut NativeTestCase,
    min_days: i64,
    max_days: i64,
) -> Result<Date, EngineError> {
    let center = days_from_civil(&Date {
        year: 2000,
        month: 1,
        day: 1,
    })
    .clamp(min_days, max_days);
    let offset = draw_i64(ntc, min_days - center, max_days - center)?;
    Ok(civil_from_days(center + offset))
}

/// The unspanned bounded time draw shared by [`generate_time`] and
/// [`generate_datetime`].
///
/// Mirrors Hypothesis's `TimeStrategy`: one integer draw of a microsecond
/// offset from `min_us`, shrinking toward `min_us` (the representable time
/// closest to midnight).
fn draw_time_in(ntc: &mut NativeTestCase, min_us: i64, max_us: i64) -> Result<Time, EngineError> {
    let offset = draw_i64(ntc, 0, max_us - min_us)?;
    Ok(time_from_us(min_us + offset))
}

/// Draw a [`Date`] in `[min, max]`, wrapped in a span so the shrinker treats
/// it as a unit. Shrinks toward 2000-01-01, or the nearest bound when that
/// is out of range.
pub fn generate_date(ntc: &mut NativeTestCase, min: Date, max: Date) -> Result<Date, EngineError> {
    validate_date("min_value", &min)?;
    validate_date("max_value", &max)?;
    let (min_days, max_days) = (days_from_civil(&min), days_from_civil(&max));
    if min_days > max_days {
        return Err(EngineError::InvalidArgument(format!(
            "generate_date requires min_value <= max_value, got [{min:?}, {max:?}]"
        )));
    }
    spanned(ntc, LABEL_DATE, |ntc| draw_date_in(ntc, min_days, max_days))
}

/// Draw a [`Time`] in `[min, max]`, wrapped in a span. Shrinks toward `min`
/// (the representable time closest to midnight).
pub fn generate_time(ntc: &mut NativeTestCase, min: Time, max: Time) -> Result<Time, EngineError> {
    validate_time("min_value", &min)?;
    validate_time("max_value", &max)?;
    let (min_us, max_us) = (time_to_us(&min), time_to_us(&max));
    if min_us > max_us {
        return Err(EngineError::InvalidArgument(format!(
            "generate_time requires min_value <= max_value, got [{min:?}, {max:?}]"
        )));
    }
    spanned(ntc, LABEL_TIME, |ntc| draw_time_in(ntc, min_us, max_us))
}

/// Draw a [`DateTime`] in `[min, max]`, wrapped in a span: a bounded date
/// draw, then a time draw whose bounds tighten to the endpoint times when
/// the drawn date lands on a boundary date.
pub fn generate_datetime(
    ntc: &mut NativeTestCase,
    min: DateTime,
    max: DateTime,
) -> Result<DateTime, EngineError> {
    validate_date("min_value.date", &min.date)?;
    validate_date("max_value.date", &max.date)?;
    validate_time("min_value.time", &min.time)?;
    validate_time("max_value.time", &max.time)?;
    let (min_days, max_days) = (days_from_civil(&min.date), days_from_civil(&max.date));
    let (min_us, max_us) = (time_to_us(&min.time), time_to_us(&max.time));
    if (min_days, min_us) > (max_days, max_us) {
        return Err(EngineError::InvalidArgument(format!(
            "generate_datetime requires min_value <= max_value, got [{min:?}, {max:?}]"
        )));
    }
    spanned(ntc, LABEL_DATETIME, |ntc| {
        let date = draw_date_in(ntc, min_days, max_days)?;
        let day = days_from_civil(&date);
        let lo = if day == min_days { min_us } else { 0 };
        let hi = if day == max_days {
            max_us
        } else {
            LAST_US_OF_DAY
        };
        let time = draw_time_in(ntc, lo, hi)?;
        Ok(DateTime { date, time })
    })
}

/// Draw a UUID's 16 big-endian bytes, wrapped in a span. When `version` is
/// set, the RFC 4122 version and variant nibbles are forced accordingly.
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
pub fn generate_uuid(
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
    spanned(ntc, LABEL_UUID, |ntc| {
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
    })
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
