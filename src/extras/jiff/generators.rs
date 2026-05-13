use std::str::FromStr;

use ciborium::Value;
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{Offset, TimeZone};
use jiff::{SignedDuration, Span, Timestamp, Zoned};

use crate::cbor_utils::{cbor_array, cbor_map};
use crate::generators::{BasicGenerator, BoxedGenerator, Generator, deserialize_value};

/// Generator for [`jiff::civil::Date`] values. Created by [`dates()`].
pub struct DateGenerator;

impl Generator<Date> for DateGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, Date>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "date"}, |raw| {
            let s: String = deserialize_value(raw);
            Date::from_str(&s).unwrap()
        }))
    }
}

/// Generate [`jiff::civil::Date`] values.
///
/// See [`DateGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let d = tc.draw(jiff_gs::dates());
///     assert!(d.year() >= 1);
/// }
/// ```
pub fn dates() -> DateGenerator {
    DateGenerator
}

/// Generator for [`jiff::civil::Time`] values. Created by [`times()`].
pub struct TimeGenerator;

impl Generator<Time> for TimeGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, Time>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "time"}, |raw| {
            let s: String = deserialize_value(raw);
            Time::from_str(&s).unwrap()
        }))
    }
}

/// Generate [`jiff::civil::Time`] values.
///
/// See [`TimeGenerator`] for builder methods.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::jiff as jiff_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let t = tc.draw(jiff_gs::times());
///     assert!(t.hour() >= 0 && t.hour() <= 23);
/// }
/// ```
pub fn times() -> TimeGenerator {
    TimeGenerator
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
    fn as_basic(&self) -> Option<BasicGenerator<'_, DateTime>> {
        assert!(
            self.min_value <= self.max_value,
            "Cannot have max_value < min_value"
        );
        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => datetime_to_nanos(self.min_value),
            "max_value" => datetime_to_nanos(self.max_value),
        };
        Some(BasicGenerator::new(schema, |raw| {
            let n: i128 = deserialize_value(raw);
            nanos_to_datetime(n)
        }))
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

    fn build_schema(&self) -> Value {
        assert!(
            self.min_value <= self.max_value,
            "Cannot have max_value < min_value"
        );
        cbor_map! {
            "type" => "integer",
            "min_value" => self.min_value.as_nanosecond(),
            "max_value" => self.max_value.as_nanosecond(),
        }
    }
}

impl Generator<Timestamp> for TimestampGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, Timestamp>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let nanos: i128 = deserialize_value(raw);
            Timestamp::from_nanosecond(nanos).unwrap()
        }))
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

    fn build_schema(&self) -> Value {
        assert!(
            self.min_nanos <= self.max_nanos,
            "Cannot have max_nanoseconds < min_nanoseconds"
        );
        assert!(
            self.min_nanos >= -SPAN_NANOS_LIMIT,
            "min_nanoseconds must be >= -i64::MAX (the Span nanosecond limit)"
        );
        cbor_map! {
            "type" => "integer",
            "min_value" => self.min_nanos,
            "max_value" => self.max_nanos,
        }
    }
}

impl Generator<Span> for SpanGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, Span>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let nanos: i64 = deserialize_value(raw);
            Span::new().try_nanoseconds(nanos).unwrap()
        }))
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

    fn build_schema(&self) -> Value {
        assert!(
            self.min_value <= self.max_value,
            "Cannot have max_value < min_value"
        );
        cbor_map! {
            "type" => "integer",
            "min_value" => self.min_value.as_nanos(),
            "max_value" => self.max_value.as_nanos(),
        }
    }
}

impl Generator<SignedDuration> for SignedDurationGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, SignedDuration>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let nanos: i128 = deserialize_value(raw);
            nanos_to_signed_duration(nanos)
        }))
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

    fn build_schema(&self) -> Value {
        assert!(
            self.min_value.seconds() <= self.max_value.seconds(),
            "Cannot have max_value < min_value"
        );
        cbor_map! {
            "type" => "integer",
            "min_value" => self.min_value.seconds(),
            "max_value" => self.max_value.seconds(),
        }
    }
}

impl Generator<Offset> for OffsetGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, Offset>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let secs: i32 = deserialize_value(raw);
            Offset::from_seconds(secs).unwrap()
        }))
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
    fn as_basic(&self) -> Option<BasicGenerator<'_, Zoned>> {
        let ts_basic = self.timestamp_gen.as_basic()?;
        let tz_basic = self.timezone_gen.as_basic()?;
        let schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![
                ts_basic.schema().clone(),
                tz_basic.schema().clone(),
            ],
        };
        Some(BasicGenerator::new(schema, move |raw| {
            let [ts_raw, tz_raw]: [Value; 2] = raw.into_array().unwrap().try_into().unwrap();
            let ts = ts_basic.parse_raw(ts_raw);
            let tz = tz_basic.parse_raw(tz_raw);
            Zoned::new(ts, tz)
        }))
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
