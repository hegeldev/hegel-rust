use super::{Generator, TestCase, integers};
use crate::test_case::invalid_argument;
use std::time::Duration;

/// Generator for [`Duration`] values. Created by [`durations()`].
///
/// Internally generates nanoseconds as a `u64`, so the maximum representable
/// duration is approximately 584 years (`u64::MAX` nanoseconds).
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
}

impl Generator<Duration> for DurationGenerator {
    fn do_draw(&self, tc: &TestCase) -> Duration {
        if self.min_nanos > self.max_nanos {
            invalid_argument!("Cannot have max_value < min_value");
        }
        let nanos = integers::<u64>()
            .min_value(self.min_nanos)
            .max_value(self.max_nanos)
            .do_draw(tc);
        Duration::from_nanos(nanos)
    }
}

/// Generate [`Duration`] values.
/// By default, generates durations from zero up to `u64::MAX` nanoseconds
/// (approximately 584 years). Use builder methods `min_value` and `max_value`
/// to constrain the range. See [`DurationGenerator`] for more details.
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

fn duration_to_nanos(d: Duration) -> u64 {
    d.as_nanos().try_into().unwrap_or(u64::MAX)
}
