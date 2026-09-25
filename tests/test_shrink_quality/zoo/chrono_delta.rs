//! A model of chrono's `TimeDelta` — just enough of it, after chrono 0.4's `time_delta.rs` at
//! the zoo's pinned commit — so the chrono cases run without chrono. `arb_delta` is the zoo
//! test's generator, draw for draw.

use hegel::TestCase;
use hegel::generators as gs;

const NANOS_PER_SEC: i32 = 1_000_000_000;
const NANOS_PER_MILLI: i32 = 1_000_000;
const MILLIS_PER_SEC: i64 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeDelta {
    secs: i64,
    nanos: i32,
}

impl TimeDelta {
    pub const MIN: TimeDelta = TimeDelta {
        secs: -i64::MAX / MILLIS_PER_SEC - 1,
        nanos: NANOS_PER_SEC + (-i64::MAX % MILLIS_PER_SEC) as i32 * NANOS_PER_MILLI,
    };
    pub const MAX: TimeDelta = TimeDelta {
        secs: i64::MAX / MILLIS_PER_SEC,
        nanos: (i64::MAX % MILLIS_PER_SEC) as i32 * NANOS_PER_MILLI,
    };

    pub const fn zero() -> TimeDelta {
        TimeDelta { secs: 0, nanos: 0 }
    }

    pub fn new(secs: i64, nanos: u32) -> Option<TimeDelta> {
        if nanos >= NANOS_PER_SEC as u32 {
            return None;
        }
        let d = TimeDelta {
            secs,
            nanos: nanos as i32,
        };
        (d >= Self::MIN && d <= Self::MAX).then_some(d)
    }

    pub fn nanoseconds(n: i64) -> TimeDelta {
        let secs = n.div_euclid(NANOS_PER_SEC as i64);
        let nanos = n.rem_euclid(NANOS_PER_SEC as i64) as i32;
        TimeDelta { secs, nanos }
    }

    pub fn try_seconds(s: i64) -> Option<TimeDelta> {
        Self::new(s, 0)
    }

    pub fn try_milliseconds(ms: i64) -> Option<TimeDelta> {
        let secs = ms.div_euclid(MILLIS_PER_SEC);
        let nanos = ms.rem_euclid(MILLIS_PER_SEC) as i32 * NANOS_PER_MILLI;
        Self::new(secs, nanos as u32)
    }

    pub fn try_days(d: i64) -> Option<TimeDelta> {
        Self::new(d * 86_400, 0)
    }

    /// chrono's `checked_mul`: guards the seconds against `i64`, not against `MAX`/`MIN`.
    pub fn checked_mul(&self, rhs: i32) -> Option<TimeDelta> {
        let total_nanos = self.nanos as i64 * rhs as i64;
        let extra_secs = total_nanos.div_euclid(NANOS_PER_SEC as i64);
        let nanos = total_nanos.rem_euclid(NANOS_PER_SEC as i64);
        let secs: i128 = self.secs as i128 * rhs as i128 + extra_secs as i128;
        if secs <= i64::MIN as i128 || secs >= i64::MAX as i128 {
            return None;
        }
        Some(TimeDelta {
            secs: secs as i64,
            nanos: nanos as i32,
        })
    }

    /// chrono's `checked_div`: floors the carried seconds' share and the nanoseconds' share
    /// separately, so the result can be one nanosecond short.
    pub fn checked_div(&self, rhs: i32) -> Option<TimeDelta> {
        if rhs == 0 {
            return None;
        }
        let secs = self.secs / rhs as i64;
        let carry = self.secs % rhs as i64;
        let extra_nanos = carry * NANOS_PER_SEC as i64 / rhs as i64;
        let nanos = self.nanos / rhs + extra_nanos as i32;
        let (secs, nanos) = match nanos {
            i32::MIN..=-1 => (secs - 1, nanos + NANOS_PER_SEC),
            NANOS_PER_SEC..=i32::MAX => (secs + 1, nanos - NANOS_PER_SEC),
            _ => (secs, nanos),
        };
        Some(TimeDelta { secs, nanos })
    }

    pub fn abs(&self) -> TimeDelta {
        if self.secs < 0 && self.nanos != 0 {
            TimeDelta {
                secs: -(self.secs + 1),
                nanos: NANOS_PER_SEC - self.nanos,
            }
        } else {
            TimeDelta {
                secs: self.secs.wrapping_abs(),
                nanos: self.nanos,
            }
        }
    }

    pub fn total_nanos(&self) -> i128 {
        self.secs as i128 * NANOS_PER_SEC as i128 + self.nanos as i128
    }

    pub fn from_total_nanos(n: i128) -> Option<TimeDelta> {
        if n < Self::MIN.total_nanos() || n > Self::MAX.total_nanos() {
            return None;
        }
        Self::new(
            n.div_euclid(NANOS_PER_SEC as i128) as i64,
            n.rem_euclid(NANOS_PER_SEC as i128) as u32,
        )
    }
}

/// The zoo test's `arb_delta`, draw for draw: a five-way branch, then the branch's values.
pub fn arb_delta(tc: &TestCase) -> TimeDelta {
    match tc.draw_silent(gs::integers::<u8>().min_value(0).max_value(4)) {
        0 => TimeDelta::nanoseconds(
            tc.draw_silent(
                gs::integers::<i64>()
                    .min_value(-1_000_000_000_000)
                    .max_value(1_000_000_000_000),
            ),
        ),
        1 => TimeDelta::try_seconds(
            tc.draw_silent(
                gs::integers::<i64>()
                    .min_value(-100_000_000)
                    .max_value(100_000_000),
            ),
        )
        .unwrap(),
        2 => TimeDelta::try_milliseconds(
            tc.draw_silent(
                gs::integers::<i64>()
                    .min_value(-i64::MAX)
                    .max_value(i64::MAX),
            ),
        )
        .unwrap(),
        3 => TimeDelta::new(
            tc.draw_silent(
                gs::integers::<i64>()
                    .min_value(-10_000_000)
                    .max_value(10_000_000),
            ),
            tc.draw_silent(gs::integers::<u32>().min_value(0).max_value(999_999_999)),
        )
        .unwrap(),
        _ => tc.draw_silent(gs::sampled_from(vec![
            TimeDelta::zero(),
            TimeDelta::MIN,
            TimeDelta::MAX,
            TimeDelta::nanoseconds(1),
            TimeDelta::nanoseconds(-1),
            TimeDelta::try_days(1).unwrap(),
            TimeDelta::try_days(-1).unwrap(),
            TimeDelta::nanoseconds(i64::MAX),
            TimeDelta::nanoseconds(i64::MIN),
        ])),
    }
}
