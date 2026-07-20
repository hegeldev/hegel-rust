use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{Offset, TimeZone};
use jiff::{SignedDuration, Span, Timestamp, Zoned};

use crate::generators::{BoxedGenerator, Generator, TestCase, integers};
use crate::test_case::invalid_argument;

/// Convert a [`Date`] to the engine's date struct. Every jiff `Date` fits:
/// jiff spans years ±9999 and the engine accepts ±999999.
fn hegel_date(d: Date) -> hegel_c::hegel_date_t {
    hegel_c::hegel_date_t {
        year: i32::from(d.year()),
        month: d.month() as u8,
        day: d.day() as u8,
    }
}

/// Generator for [`jiff::civil::Date`] values. Created by [`dates()`].
pub struct DateGenerator {
    min_value: Date,
    max_value: Date,
}

impl DateGenerator {
    /// Set the minimum date (inclusive).
    pub fn min_value(mut self, min: Date) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum date (inclusive).
    pub fn max_value(mut self, max: Date) -> Self {
        self.max_value = max;
        self
    }
}

impl Generator<Date> for DateGenerator {
    fn do_draw(&self, tc: &TestCase) -> Date {
        if self.min_value > self.max_value {
            invalid_argument!("Cannot have max_value < min_value");
        }
        let d = tc.generate_date(hegel_date(self.min_value), hegel_date(self.max_value));
        Date::new(d.year as i16, d.month as i8, d.day as i8).unwrap()
    }
}

/// Generate [`jiff::civil::Date`] values.
///
/// Defaults span years 1–9999 (`0001-01-01` through `9999-12-31`). Use the
/// builder methods to constrain the range, or to widen it down to jiff's
/// minimum of `-9999-01-01`.
///
/// See [`DateGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
/// use jiff::civil::Date;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let min = Date::constant(2024, 1, 1);
///     let d = tc.draw(jiff_gs::dates().min_value(min));
///     assert!(d >= min);
/// }
/// ```
pub fn dates() -> DateGenerator {
    DateGenerator {
        min_value: Date::constant(1, 1, 1),
        max_value: Date::constant(9999, 12, 31),
    }
}

/// Total nanoseconds from midnight for a [`Time`].
fn time_total_nanos(t: Time) -> i64 {
    (i64::from(t.hour()) * 3_600 + i64::from(t.minute()) * 60 + i64::from(t.second()))
        * 1_000_000_000
        + i64::from(t.subsec_nanosecond())
}

/// Convert whole microseconds from midnight to the engine's time struct.
fn hegel_time(total_micros: i64) -> hegel_c::hegel_time_t {
    hegel_c::hegel_time_t {
        hour: (total_micros / 3_600_000_000) as u8,
        minute: (total_micros / 60_000_000 % 60) as u8,
        second: (total_micros / 1_000_000 % 60) as u8,
        microsecond: (total_micros % 1_000_000) as u32,
    }
}

/// Generator for [`jiff::civil::Time`] values. Created by [`times()`].
pub struct TimeGenerator {
    min_value: Time,
    max_value: Time,
}

impl TimeGenerator {
    /// Set the minimum time (inclusive).
    pub fn min_value(mut self, min: Time) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum time (inclusive).
    pub fn max_value(mut self, max: Time) -> Self {
        self.max_value = max;
        self
    }
}

impl Generator<Time> for TimeGenerator {
    fn do_draw(&self, tc: &TestCase) -> Time {
        if self.min_value > self.max_value {
            invalid_argument!("Cannot have max_value < min_value");
        }
        // Generated times are whole microseconds, so round the bounds
        // inward: min up, max down (totals are non-negative). That can empty
        // an in-order range whose bounds sit between two consecutive
        // microseconds.
        let min_micros = (time_total_nanos(self.min_value) + 999) / 1_000;
        let max_micros = time_total_nanos(self.max_value) / 1_000;
        if min_micros > max_micros {
            invalid_argument!(
                "times() generates whole-microsecond values, and no whole microsecond \
                 lies between min_value and max_value"
            );
        }
        let t = tc.generate_time(hegel_time(min_micros), hegel_time(max_micros));
        Time::new(
            t.hour as i8,
            t.minute as i8,
            t.second as i8,
            (t.microsecond * 1000) as i32,
        )
        .unwrap()
    }
}

/// Generate [`jiff::civil::Time`] values.
///
/// Generated times have whole-microsecond precision (`subsec_nanosecond()`
/// is always a multiple of 1000). Bounds may carry sub-microsecond
/// components; they are honoured by rounding inward to the enclosed
/// microsecond range.
///
/// See [`TimeGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
/// use jiff::civil::Time;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let max = Time::constant(12, 0, 0, 0);
///     let t = tc.draw(jiff_gs::times().max_value(max));
///     assert!(t <= max);
/// }
/// ```
pub fn times() -> TimeGenerator {
    TimeGenerator {
        min_value: Time::MIN,
        max_value: Time::MAX,
    }
}

/// Convert a [`DateTime`] to nanoseconds since the Unix epoch, treating it
/// as UTC. The wire format for [`DateTimeGenerator`] is a stable monotonic
/// encoding of a civil datetime; UTC nanos give us that for free.
///
/// Bounds must fit within `Timestamp::MIN..=Timestamp::MAX`, which is slightly
/// narrower than `DateTime::MIN..=DateTime::MAX` (jiff reserves a ~26-hour
/// buffer at each end for offset conversions).
fn datetime_to_nanos(dt: DateTime) -> i128 {
    dt.to_zoned(TimeZone::UTC)
        .expect("DateTime bound out of jiff::Timestamp range")
        .timestamp()
        .as_nanosecond()
}

/// Inverse of [`datetime_to_nanos`].
fn nanos_to_datetime(n: i128) -> DateTime {
    Timestamp::from_nanosecond(n)
        .unwrap()
        .to_zoned(TimeZone::UTC)
        .datetime()
}

/// Generator for [`jiff::civil::DateTime`] values. Created by [`datetimes()`].
pub struct DateTimeGenerator {
    min_value: DateTime,
    max_value: DateTime,
}

impl DateTimeGenerator {
    /// Set the minimum datetime (inclusive).
    pub fn min_value(mut self, min: DateTime) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum datetime (inclusive).
    pub fn max_value(mut self, max: DateTime) -> Self {
        self.max_value = max;
        self
    }
}

impl Generator<DateTime> for DateTimeGenerator {
    fn do_draw(&self, tc: &TestCase) -> DateTime {
        if self.min_value > self.max_value {
            invalid_argument!("Cannot have max_value < min_value");
        }
        let n = integers::<i128>()
            .min_value(datetime_to_nanos(self.min_value))
            .max_value(datetime_to_nanos(self.max_value))
            .do_draw(tc);
        nanos_to_datetime(n)
    }
}

/// Generate [`jiff::civil::DateTime`] values.
///
/// See [`DateTimeGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
/// use jiff::civil::DateTime;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let min = DateTime::constant(2024, 1, 1, 0, 0, 0, 0);
///     let dt = tc.draw(jiff_gs::datetimes().min_value(min));
///     assert!(dt >= min);
/// }
/// ```
pub fn datetimes() -> DateTimeGenerator {
    DateTimeGenerator {
        min_value: Timestamp::MIN.to_zoned(TimeZone::UTC).datetime(),
        max_value: Timestamp::MAX.to_zoned(TimeZone::UTC).datetime(),
    }
}

/// Generator for [`jiff::Timestamp`] values. Created by [`timestamps()`].
pub struct TimestampGenerator {
    min_value: Timestamp,
    max_value: Timestamp,
}

impl TimestampGenerator {
    /// Set the minimum timestamp (inclusive).
    pub fn min_value(mut self, min: Timestamp) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum timestamp (inclusive).
    pub fn max_value(mut self, max: Timestamp) -> Self {
        self.max_value = max;
        self
    }
}

impl Generator<Timestamp> for TimestampGenerator {
    fn do_draw(&self, tc: &TestCase) -> Timestamp {
        if self.min_value > self.max_value {
            invalid_argument!("Cannot have max_value < min_value");
        }
        let nanos = integers::<i128>()
            .min_value(self.min_value.as_nanosecond())
            .max_value(self.max_value.as_nanosecond())
            .do_draw(tc);
        Timestamp::from_nanosecond(nanos).unwrap()
    }
}

/// Generate [`jiff::Timestamp`] values.
///
/// See [`TimestampGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
/// use jiff::Timestamp;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let t = tc.draw(jiff_gs::timestamps()
///         .min_value(Timestamp::UNIX_EPOCH));
///     assert!(t >= Timestamp::UNIX_EPOCH);
/// }
/// ```
pub fn timestamps() -> TimestampGenerator {
    TimestampGenerator {
        min_value: Timestamp::MIN,
        max_value: Timestamp::MAX,
    }
}

/// The largest absolute nanosecond magnitude jiff allows in a `Span`.
const SPAN_NANOS_LIMIT: i64 = i64::MAX;

/// Generator for [`jiff::Span`] values. Created by [`spans()`].
pub struct SpanGenerator {
    min_nanos: i64,
    max_nanos: i64,
}

impl SpanGenerator {
    /// Set the minimum number of nanoseconds in the generated span.
    pub fn min_nanoseconds(mut self, min: i64) -> Self {
        self.min_nanos = min;
        self
    }

    /// Set the maximum number of nanoseconds in the generated span.
    pub fn max_nanoseconds(mut self, max: i64) -> Self {
        self.max_nanos = max;
        self
    }
}

impl Generator<Span> for SpanGenerator {
    fn do_draw(&self, tc: &TestCase) -> Span {
        if self.min_nanos > self.max_nanos {
            invalid_argument!("Cannot have max_nanoseconds < min_nanoseconds");
        }
        if self.min_nanos < -SPAN_NANOS_LIMIT {
            invalid_argument!("min_nanoseconds must be >= -i64::MAX (the Span nanosecond limit)");
        }
        let nanos = integers::<i64>()
            .min_value(self.min_nanos)
            .max_value(self.max_nanos)
            .do_draw(tc);
        Span::new().try_nanoseconds(nanos).unwrap()
    }
}

/// Generate [`jiff::Span`] values.
///
/// See [`SpanGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let s = tc.draw(jiff_gs::spans().min_nanoseconds(0));
///     assert!(s.get_nanoseconds() >= 0);
/// }
/// ```
pub fn spans() -> SpanGenerator {
    SpanGenerator {
        min_nanos: -SPAN_NANOS_LIMIT,
        max_nanos: SPAN_NANOS_LIMIT,
    }
}

/// Inverse of [`SignedDuration::as_nanos`].
fn nanos_to_signed_duration(n: i128) -> SignedDuration {
    let secs = (n / 1_000_000_000) as i64;
    let subnanos = (n % 1_000_000_000) as i32;
    SignedDuration::new(secs, subnanos)
}

/// Generator for [`jiff::SignedDuration`] values. Created by [`signed_durations()`].
pub struct SignedDurationGenerator {
    min_value: SignedDuration,
    max_value: SignedDuration,
}

impl SignedDurationGenerator {
    /// Set the minimum duration (inclusive).
    pub fn min_value(mut self, min: SignedDuration) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum duration (inclusive).
    pub fn max_value(mut self, max: SignedDuration) -> Self {
        self.max_value = max;
        self
    }
}

impl Generator<SignedDuration> for SignedDurationGenerator {
    fn do_draw(&self, tc: &TestCase) -> SignedDuration {
        if self.min_value > self.max_value {
            invalid_argument!("Cannot have max_value < min_value");
        }
        let nanos = integers::<i128>()
            .min_value(self.min_value.as_nanos())
            .max_value(self.max_value.as_nanos())
            .do_draw(tc);
        nanos_to_signed_duration(nanos)
    }
}

/// Generate [`jiff::SignedDuration`] values.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
/// use jiff::SignedDuration;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let d = tc.draw(jiff_gs::signed_durations().min_value(SignedDuration::ZERO));
///     assert!(d.as_nanos() >= 0);
/// }
/// ```
pub fn signed_durations() -> SignedDurationGenerator {
    SignedDurationGenerator {
        min_value: SignedDuration::MIN,
        max_value: SignedDuration::MAX,
    }
}

/// The minimum representable offset in seconds (-25:59:59).
const OFFSET_MIN_SECS: i32 = -93_599;
/// The maximum representable offset in seconds (+25:59:59).
const OFFSET_MAX_SECS: i32 = 93_599;

/// Generator for [`jiff::tz::Offset`] values. Created by [`offsets()`].
pub struct OffsetGenerator {
    min_value: Offset,
    max_value: Offset,
}

impl OffsetGenerator {
    /// Set the minimum offset (inclusive).
    pub fn min_value(mut self, min: Offset) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum offset (inclusive).
    pub fn max_value(mut self, max: Offset) -> Self {
        self.max_value = max;
        self
    }
}

impl Generator<Offset> for OffsetGenerator {
    fn do_draw(&self, tc: &TestCase) -> Offset {
        if self.min_value.seconds() > self.max_value.seconds() {
            invalid_argument!("Cannot have max_value < min_value");
        }
        let secs = integers::<i32>()
            .min_value(self.min_value.seconds())
            .max_value(self.max_value.seconds())
            .do_draw(tc);
        Offset::from_seconds(secs).unwrap()
    }
}

/// Generate [`jiff::tz::Offset`] values.
///
/// See [`OffsetGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
/// use jiff::tz::Offset;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let o = tc.draw(jiff_gs::offsets()
///         .min_value(Offset::ZERO));
///     assert!(o.seconds() >= 0);
/// }
/// ```
pub fn offsets() -> OffsetGenerator {
    OffsetGenerator {
        min_value: Offset::from_seconds(OFFSET_MIN_SECS).unwrap(),
        max_value: Offset::from_seconds(OFFSET_MAX_SECS).unwrap(),
    }
}

/// Generator for [`jiff::Zoned`] values. Created by [`zoneds()`].
pub struct ZonedGenerator<TS = TimestampGenerator, TZ = BoxedGenerator<'static, TimeZone>> {
    timestamp_gen: TS,
    timezone_gen: TZ,
}

impl<TS, TZ> ZonedGenerator<TS, TZ> {
    /// Replace the timestamp generator.
    pub fn timestamps<TS2>(self, timestamp_gen: TS2) -> ZonedGenerator<TS2, TZ>
    where
        TS2: Generator<Timestamp>,
    {
        ZonedGenerator {
            timestamp_gen,
            timezone_gen: self.timezone_gen,
        }
    }

    /// Replace the timezone generator.
    pub fn timezones<TZ2>(self, timezone_gen: TZ2) -> ZonedGenerator<TS, TZ2>
    where
        TZ2: Generator<TimeZone>,
    {
        ZonedGenerator {
            timestamp_gen: self.timestamp_gen,
            timezone_gen,
        }
    }
}

impl<TS, TZ> Generator<Zoned> for ZonedGenerator<TS, TZ>
where
    TS: Generator<Timestamp>,
    TZ: Generator<TimeZone>,
{
    fn do_draw(&self, tc: &TestCase) -> Zoned {
        let (ts, tz) =
            crate::generators::tuples2(&self.timestamp_gen, &self.timezone_gen).do_draw(tc);
        Zoned::new(ts, tz)
    }
}

/// Generate [`jiff::Zoned`] values.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
/// use hegel::generators as gs;
/// use jiff::tz::TimeZone;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let _ = tc.draw(jiff_gs::zoneds());
///     let _ = tc.draw(jiff_gs::zoneds().timezones(gs::just(TimeZone::UTC)));
/// }
/// ```
pub fn zoneds() -> ZonedGenerator {
    use crate::generators::DefaultGenerator;
    ZonedGenerator {
        timestamp_gen: timestamps(),
        timezone_gen: <TimeZone as DefaultGenerator>::default_generator(),
    }
}
