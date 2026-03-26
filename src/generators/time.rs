use super::{BasicGenerator, Generator, TestCase};
use crate::cbor_utils::cbor_map;
use std::time::{Duration, Instant};

/// Generator for [`Duration`] values. Created by [`durations()`].
///
/// Internally generates nanoseconds as a `u64`, so the maximum representable
/// duration is approximately 584 years (`u64::MAX` nanoseconds).
/// Use `min_value` and `max_value` to constrain the range.
pub struct DurationGenerator {
    min_nanos: u64,
    max_nanos: u64,
}

impl DurationGenerator {
    /// Set the minimum duration (inclusive).
    pub fn min_value(mut self, min: Duration) -> Self {
        self.min_nanos = duration_to_nanos(min);
        self
    }

    /// Set the maximum duration (inclusive).
    pub fn max_value(mut self, max: Duration) -> Self {
        self.max_nanos = duration_to_nanos(max);
        self
    }

    fn build_schema(&self) -> ciborium::Value {
        assert!(
            self.min_nanos <= self.max_nanos,
            "Cannot have max_value < min_value"
        );
        cbor_map! {
            "type" => "integer",
            "min_value" => self.min_nanos,
            "max_value" => self.max_nanos
        }
    }
}

impl Generator<Duration> for DurationGenerator {
    fn do_draw(&self, tc: &TestCase) -> Duration {
        let nanos: u64 = super::generate_from_schema(tc, &self.build_schema());
        Duration::from_nanos(nanos)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Duration>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            let nanos: u64 = super::deserialize_value(raw);
            Duration::from_nanos(nanos)
        }))
    }
}

/// Generate [`Duration`] values.
///
/// By default, generates durations from zero up to `u64::MAX` nanoseconds
/// (approximately 584 years). Use `min_value` and `max_value` to constrain
/// the range.
///
/// # Example
///
/// ```no_run
/// use std::time::Duration;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let d = tc.draw(hegel::generators::durations()
///         .max_value(Duration::from_secs(60)));
///     assert!(d <= Duration::from_secs(60));
/// }
/// ```
pub fn durations() -> DurationGenerator {
    DurationGenerator {
        min_nanos: 0,
        max_nanos: u64::MAX,
    }
}

/// Generator for [`Instant`] values. Created by [`instants()`].
///
/// Generates instants by adding a random [`Duration`] offset to the current
/// time (`Instant::now()`). Since `Instant` values are inherently tied to the
/// monotonic clock, each test run produces different absolute values.
pub struct InstantGenerator {
    max_offset_nanos: u64,
}

impl InstantGenerator {
    /// Set the maximum offset from `Instant::now()` (inclusive).
    pub fn max_offset(mut self, max: Duration) -> Self {
        self.max_offset_nanos = duration_to_nanos(max);
        self
    }
}

impl Generator<Instant> for InstantGenerator {
    fn do_draw(&self, tc: &TestCase) -> Instant {
        let schema = cbor_map! {
            "type" => "integer",
            "min_value" => 0u64,
            "max_value" => self.max_offset_nanos
        };
        let nanos: u64 = super::generate_from_schema(tc, &schema);
        Instant::now() + Duration::from_nanos(nanos)
    }
}

/// Generate [`Instant`] values.
///
/// Produces instants offset from `Instant::now()` by a random duration.
/// The default maximum offset is one hour. Use `max_offset` to change it.
///
/// Since `Instant` is tied to the monotonic clock, generated values differ
/// between test runs. This generator is most useful for testing code that
/// computes differences between instants.
///
/// # Example
///
/// ```no_run
/// use std::time::Duration;
///
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let i = tc.draw(hegel::generators::instants()
///         .max_offset(Duration::from_secs(3600)));
/// }
/// ```
pub fn instants() -> InstantGenerator {
    InstantGenerator {
        max_offset_nanos: 3_600_000_000_000,
    }
}

fn duration_to_nanos(d: Duration) -> u64 {
    d.as_nanos().try_into().unwrap_or(u64::MAX)
}
