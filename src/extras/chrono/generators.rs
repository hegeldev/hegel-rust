use crate::cbor_utils::{cbor_array, cbor_map};
use crate::generators::{BasicGenerator, DefaultGenerator, Generator, TestCase};
use chrono::{
    DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, NaiveWeek, TimeDelta,
    TimeZone, Timelike, Utc, Weekday, WeekdaySet,
};
use ciborium::Value;
use std::marker::PhantomData;

/// Convert a [`NaiveTime`] to a total nanosecond count from midnight.
///
/// chrono encodes leap seconds by letting `nanosecond()` reach into
/// `[1_000_000_000, 2_000_000_000)`; those bits are preserved here so the
/// conversion is lossless and round-trips through [`total_nanos_to_time`].
fn time_to_total_nanos(t: NaiveTime) -> i64 {
    let secs = i64::from(t.num_seconds_from_midnight());
    let nanos = i64::from(t.nanosecond());
    secs * 1_000_000_000 + nanos
}

/// Inverse of [`time_to_total_nanos`].
fn total_nanos_to_time(total: i64) -> NaiveTime {
    // For leap-second values, chrono keeps secs at 86399 and pushes the extra
    // second into the nanos field. Detect that range and reconstruct in kind.
    let (secs, nanos) = if total >= 86_400 * 1_000_000_000 {
        (86_399, (total - 86_399 * 1_000_000_000) as u32)
    } else {
        (
            (total / 1_000_000_000) as u32,
            (total % 1_000_000_000) as u32,
        )
    };
    NaiveTime::from_num_seconds_from_midnight_opt(secs, nanos).unwrap()
}

/// Default upper bound for [`naive_times`]. Excludes leap-second representation
/// — users who want to generate leap seconds must opt in via [`NaiveTimeGenerator::max_value`].
fn naive_time_default_max() -> NaiveTime {
    NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap()
}

/// Generator for [`chrono::WeekdaySet`] values. Created by [`weekday_sets()`].
pub struct WeekdaySetGenerator;

impl WeekdaySetGenerator {
    fn build_schema(&self) -> Value {
        cbor_map! {
            "type" => "list",
            "unique" => true,
            "elements" => cbor_map! {
                "type" => "integer",
                "min_value" => 0u64,
                "max_value" => 6u64,
            },
            "min_size" => 0u64,
            "max_size" => 7u64,
        }
    }
}

impl Generator<WeekdaySet> for WeekdaySetGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, WeekdaySet>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let arr = raw.into_array().unwrap();
            let mut set = WeekdaySet::EMPTY;
            for v in arr {
                let n: u8 = crate::generators::deserialize_value(v);
                set.insert(Weekday::try_from(n).unwrap());
            }
            set
        }))
    }
}

/// Generate [`chrono::WeekdaySet`] values.
///
/// # Example
///
/// ```no_run
/// use hegel::extras::chrono as chrono_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let s: chrono::WeekdaySet = tc.draw(chrono_gs::weekday_sets());
///     assert!(s.len() <= 7);
/// }
/// ```
pub fn weekday_sets() -> WeekdaySetGenerator {
    WeekdaySetGenerator
}

/// Generator for [`chrono::FixedOffset`] values. Created by [`fixed_offsets()`].
pub struct FixedOffsetGenerator {
    min_value: FixedOffset,
    max_value: FixedOffset,
}

impl FixedOffsetGenerator {
    /// Set the minimum offset (inclusive).
    pub fn min_value(mut self, min: FixedOffset) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum offset (inclusive).
    pub fn max_value(mut self, max: FixedOffset) -> Self {
        self.max_value = max;
        self
    }

    fn build_schema(&self) -> Value {
        let min_secs = self.min_value.local_minus_utc();
        let max_secs = self.max_value.local_minus_utc();
        assert!(min_secs <= max_secs, "Cannot have max_value < min_value");
        cbor_map! {
            "type" => "integer",
            "min_value" => i64::from(min_secs),
            "max_value" => i64::from(max_secs),
        }
    }
}

impl Generator<FixedOffset> for FixedOffsetGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, FixedOffset>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let secs: i32 = crate::generators::deserialize_value(raw);
            FixedOffset::east_opt(secs).unwrap()
        }))
    }
}

/// Generate [`chrono::FixedOffset`] values.
///
/// # Example
///
/// ```no_run
/// use chrono::FixedOffset;
/// use hegel::extras::chrono as chrono_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let max = FixedOffset::east_opt(12 * 3600).unwrap();
///     let off = tc.draw(chrono_gs::fixed_offsets().max_value(max));
///     assert!(off.local_minus_utc() <= 12 * 3600);
/// }
/// ```
pub fn fixed_offsets() -> FixedOffsetGenerator {
    FixedOffsetGenerator {
        min_value: FixedOffset::west_opt(86_399).unwrap(),
        max_value: FixedOffset::east_opt(86_399).unwrap(),
    }
}

/// Convert a [`TimeDelta`] to a total nanosecond count.
///
/// `TimeDelta` is internally `(secs: i64, nanos: u32 in [0, 1e9))`. The total
/// nanosecond magnitude exceeds `i64`'s range by a factor of ~10^9, so we
/// widen to `i128` (which has ~10^10 of headroom past `TimeDelta::MAX`).
fn timedelta_to_nanos(d: TimeDelta) -> i128 {
    i128::from(d.num_seconds()) * 1_000_000_000 + i128::from(d.subsec_nanos())
}

/// Inverse of [`timedelta_to_nanos`].
fn nanos_to_timedelta(n: i128) -> TimeDelta {
    let secs = n.div_euclid(1_000_000_000) as i64;
    let nanos = n.rem_euclid(1_000_000_000) as u32;
    TimeDelta::new(secs, nanos).unwrap()
}

/// Generator for [`chrono::TimeDelta`] values. Created by [`time_deltas()`].
pub struct TimeDeltaGenerator {
    min_value: TimeDelta,
    max_value: TimeDelta,
}

impl TimeDeltaGenerator {
    /// Set the minimum delta (inclusive).
    pub fn min_value(mut self, min: TimeDelta) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum delta (inclusive).
    pub fn max_value(mut self, max: TimeDelta) -> Self {
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
            "min_value" => timedelta_to_nanos(self.min_value),
            "max_value" => timedelta_to_nanos(self.max_value),
        }
    }
}

impl Generator<TimeDelta> for TimeDeltaGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, TimeDelta>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let nanos: i128 = crate::generators::deserialize_value(raw);
            nanos_to_timedelta(nanos)
        }))
    }
}

/// Generate [`chrono::TimeDelta`] values.
///
/// Defaults span the full `TimeDelta::MIN..=TimeDelta::MAX` range. Use the
/// builder methods to constrain.
///
/// # Example
///
/// ```no_run
/// use chrono::TimeDelta;
/// use hegel::extras::chrono as chrono_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let max = TimeDelta::seconds(60);
///     let d = tc.draw(chrono_gs::time_deltas().min_value(TimeDelta::zero()).max_value(max));
///     assert!(d >= TimeDelta::zero() && d <= max);
/// }
/// ```
pub fn time_deltas() -> TimeDeltaGenerator {
    TimeDeltaGenerator {
        min_value: TimeDelta::MIN,
        max_value: TimeDelta::MAX,
    }
}

/// Generator for [`chrono::NaiveDate`] values. Created by [`naive_dates()`].
///
/// Internally the date is generated as a count of days from the Common Era
/// epoch (`NaiveDate::from_num_days_from_ce_opt`).
pub struct NaiveDateGenerator {
    min_value: NaiveDate,
    max_value: NaiveDate,
}

impl NaiveDateGenerator {
    /// Set the minimum date (inclusive).
    pub fn min_value(mut self, min: NaiveDate) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum date (inclusive).
    pub fn max_value(mut self, max: NaiveDate) -> Self {
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
            "min_value" => self.min_value.num_days_from_ce(),
            "max_value" => self.max_value.num_days_from_ce(),
        }
    }
}

impl Generator<NaiveDate> for NaiveDateGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, NaiveDate>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let n: i32 = crate::generators::deserialize_value(raw);
            NaiveDate::from_num_days_from_ce_opt(n).unwrap()
        }))
    }
}

/// Generate [`chrono::NaiveDate`] values.
///
/// # Example
///
/// ```no_run
/// use chrono::NaiveDate;
/// use hegel::extras::chrono as chrono_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let min = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
///     let max = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
///     let d = tc.draw(chrono_gs::naive_dates().min_value(min).max_value(max));
///     assert_eq!(d.iso_week().year(), 2024);
/// }
/// ```
pub fn naive_dates() -> NaiveDateGenerator {
    NaiveDateGenerator {
        min_value: NaiveDate::MIN,
        max_value: NaiveDate::MAX,
    }
}

/// Generator for [`chrono::NaiveTime`] values. Created by [`naive_times()`].
pub struct NaiveTimeGenerator {
    min_value: NaiveTime,
    max_value: NaiveTime,
}

impl NaiveTimeGenerator {
    /// Set the minimum time (inclusive).
    pub fn min_value(mut self, min: NaiveTime) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum time (inclusive).
    pub fn max_value(mut self, max: NaiveTime) -> Self {
        self.max_value = max;
        self
    }

    fn build_schema(&self) -> Value {
        let min_nanos = time_to_total_nanos(self.min_value);
        let max_nanos = time_to_total_nanos(self.max_value);
        assert!(min_nanos <= max_nanos, "Cannot have max_value < min_value");
        cbor_map! {
            "type" => "integer",
            "min_value" => min_nanos,
            "max_value" => max_nanos,
        }
    }
}

impl Generator<NaiveTime> for NaiveTimeGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, NaiveTime>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let n: i64 = crate::generators::deserialize_value(raw);
            total_nanos_to_time(n)
        }))
    }
}

/// Generate [`chrono::NaiveTime`] values.
///
/// # Example
///
/// ```no_run
/// use chrono::NaiveTime;
/// use hegel::extras::chrono as chrono_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let max = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
///     let t = tc.draw(chrono_gs::naive_times().max_value(max));
///     assert!(t <= max);
/// }
/// ```
pub fn naive_times() -> NaiveTimeGenerator {
    NaiveTimeGenerator {
        min_value: NaiveTime::MIN,
        max_value: naive_time_default_max(),
    }
}

/// Generator for [`chrono::NaiveDateTime`] values. Created by [`naive_datetimes()`].
pub struct NaiveDateTimeGenerator {
    min_value: NaiveDateTime,
    max_value: NaiveDateTime,
}

impl NaiveDateTimeGenerator {
    /// Set the minimum datetime (inclusive).
    pub fn min_value(mut self, min: NaiveDateTime) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum datetime (inclusive).
    pub fn max_value(mut self, max: NaiveDateTime) -> Self {
        self.max_value = max;
        self
    }
}

impl Generator<NaiveDateTime> for NaiveDateTimeGenerator {
    fn as_basic(&self) -> Option<BasicGenerator<'_, NaiveDateTime>> {
        assert!(
            self.min_value <= self.max_value,
            "Cannot have max_value < min_value"
        );
        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => datetime_to_nanos(&self.min_value.and_utc()),
            "max_value" => datetime_to_nanos(&self.max_value.and_utc()),
        };
        Some(BasicGenerator::new(schema, |raw| {
            let n: i128 = crate::generators::deserialize_value(raw);
            nanos_to_utc_datetime(n).naive_utc()
        }))
    }
}

/// Generate [`chrono::NaiveDateTime`] values.
///
/// Defaults span chrono's full `DateTime<Utc>` window —
/// [`DateTime::<Utc>::MIN_UTC`] through [`DateTime::<Utc>::MAX_UTC`].
///
/// # Example
///
/// ```no_run
/// use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
/// use hegel::extras::chrono as chrono_gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let min = NaiveDateTime::new(
///         NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
///         NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
///     );
///     let max = NaiveDateTime::new(
///         NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
///         NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap(),
///     );
///     let dt = tc.draw(chrono_gs::naive_datetimes().min_value(min).max_value(max));
///     assert!(dt >= min && dt <= max);
/// }
/// ```
pub fn naive_datetimes() -> NaiveDateTimeGenerator {
    NaiveDateTimeGenerator {
        min_value: DateTime::<Utc>::MIN_UTC.naive_utc(),
        max_value: DateTime::<Utc>::MAX_UTC.naive_utc(),
    }
}

/// Generator for [`chrono::NaiveWeek`] values. Created by [`naive_weeks()`].
pub struct NaiveWeekGenerator<S = <Weekday as DefaultGenerator>::Generator> {
    date_gen: NaiveDateGenerator,
    start_gen: S,
}

impl<S> NaiveWeekGenerator<S> {
    /// Set the minimum source date (inclusive).
    pub fn min_date(mut self, min: NaiveDate) -> Self {
        self.date_gen = self.date_gen.min_value(min);
        self
    }

    /// Set the maximum source date (inclusive).
    pub fn max_date(mut self, max: NaiveDate) -> Self {
        self.date_gen = self.date_gen.max_value(max);
        self
    }

    /// Replace the start-of-week generator.
    pub fn weekday_starts<S2>(self, start_gen: S2) -> NaiveWeekGenerator<S2>
    where
        S2: Generator<Weekday>,
    {
        NaiveWeekGenerator {
            date_gen: self.date_gen,
            start_gen,
        }
    }
}

impl<S: Generator<Weekday>> Generator<NaiveWeek> for NaiveWeekGenerator<S> {
    fn as_basic(&self) -> Option<BasicGenerator<'_, NaiveWeek>> {
        let date_basic = self.date_gen.as_basic()?;
        let start_basic = self.start_gen.as_basic()?;
        let schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![
                date_basic.schema().clone(),
                start_basic.schema().clone(),
            ],
        };
        Some(BasicGenerator::new(schema, move |raw| {
            let [d_raw, s_raw]: [Value; 2] = raw.into_array().unwrap().try_into().unwrap();
            let date = date_basic.parse_raw(d_raw);
            let start = start_basic.parse_raw(s_raw);
            date.week(start)
        }))
    }
}

/// Generate [`chrono::NaiveWeek`] values.
///
/// Each draw produces a 7-day window around a randomly chosen [`NaiveDate`],
/// with the start-of-week [`Weekday`] also drawn at random (full 7-day range
/// by default). Use [`weekday_starts`](NaiveWeekGenerator::weekday_starts) to substitute a
/// different start-day strategy — e.g. `gs::just(Weekday::Sun)` for Sunday-only.
///
/// # Example
///
/// ```no_run
/// use chrono::Weekday;
/// use hegel::extras::chrono as chrono_gs;
/// use hegel::generators as gs;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     // Default: random start day per draw
///     let _ = tc.draw(chrono_gs::naive_weeks());
///
///     // All Sunday-start weeks
///     let _ = tc.draw(chrono_gs::naive_weeks().weekday_starts(gs::just(Weekday::Sun)));
/// }
/// ```
pub fn naive_weeks() -> NaiveWeekGenerator {
    NaiveWeekGenerator {
        date_gen: naive_dates(),
        start_gen: Weekday::default_generator(),
    }
}

/// Convert a [`DateTime`] to a total nanosecond count from the Unix epoch.
fn datetime_to_nanos<Tz: TimeZone>(dt: &DateTime<Tz>) -> i128 {
    i128::from(dt.timestamp()) * 1_000_000_000 + i128::from(dt.timestamp_subsec_nanos())
}

/// Inverse of [`datetime_to_nanos`], producing a `DateTime<Utc>`.
fn nanos_to_utc_datetime(n: i128) -> DateTime<Utc> {
    let secs = n.div_euclid(1_000_000_000) as i64;
    let nsecs = n.rem_euclid(1_000_000_000) as u32;
    DateTime::<Utc>::from_timestamp(secs, nsecs).unwrap()
}

/// Generator for [`chrono::DateTime`] values. Created by [`datetimes()`].
pub struct DateTimeGenerator<G = FixedOffsetGenerator, Tz: TimeZone = FixedOffset> {
    tz_gen: G,
    min_value: NaiveDateTime,
    max_value: NaiveDateTime,
    _phantom: PhantomData<fn() -> Tz>,
}

impl<G, Tz: TimeZone> DateTimeGenerator<G, Tz> {
    /// Set the minimum wall-clock datetime (inclusive).
    pub fn min_value(mut self, min: NaiveDateTime) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum wall-clock datetime (inclusive).
    pub fn max_value(mut self, max: NaiveDateTime) -> Self {
        self.max_value = max;
        self
    }

    /// Use the specified timezone generator.
    ///
    /// The returned generator has output type `DateTime<Tz2>` where `Tz2`
    /// is the timezone type produced by `tz_gen`. Wall-clock bounds are
    /// timezone-agnostic and carry across.
    pub fn timezones<G2, Tz2>(self, tz_gen: G2) -> DateTimeGenerator<G2, Tz2>
    where
        G2: Generator<Tz2>,
        Tz2: TimeZone,
    {
        DateTimeGenerator {
            tz_gen,
            min_value: self.min_value,
            max_value: self.max_value,
            _phantom: PhantomData,
        }
    }
}

impl<G, Tz> Generator<DateTime<Tz>> for DateTimeGenerator<G, Tz>
where
    G: Generator<Tz>,
    Tz: TimeZone + Send + Sync + 'static,
{
    fn do_draw(&self, tc: &TestCase) -> DateTime<Tz> {
        let naive = naive_datetimes()
            .min_value(self.min_value)
            .max_value(self.max_value)
            .do_draw(tc);
        let tz = self.tz_gen.do_draw(tc);
        match tz.from_local_datetime(&naive).earliest() {
            Some(dt) => dt,
            None => {
                tc.assume(false);
                unreachable!()
            }
        }
    }
}

/// Generate [`chrono::DateTime`] values.
///
/// ```no_run
/// use chrono::{FixedOffset, Utc};
/// use hegel::extras::chrono as chrono_gs;
/// use hegel::generators as gs;
///
/// // Default: DateTime<FixedOffset> with varying offsets
/// let g = chrono_gs::datetimes();
///
/// // All UTC
/// let g = chrono_gs::datetimes().timezones(gs::just(Utc));
///
/// // All in a single fixed offset
/// let off = FixedOffset::east_opt(9 * 3600).unwrap();
/// let g = chrono_gs::datetimes().timezones(gs::just(off));
///
/// // Constrain offsets to a sub-range
/// let g = chrono_gs::datetimes()
///     .timezones(chrono_gs::fixed_offsets().min_value(off));
/// ```
pub fn datetimes() -> DateTimeGenerator<FixedOffsetGenerator, FixedOffset> {
    DateTimeGenerator {
        tz_gen: fixed_offsets(),
        min_value: DateTime::<Utc>::MIN_UTC.naive_utc(),
        max_value: DateTime::<Utc>::MAX_UTC.naive_utc(),
        _phantom: PhantomData,
    }
}
