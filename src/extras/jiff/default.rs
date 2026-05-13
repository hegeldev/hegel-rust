use jiff::civil::{Date, DateTime, ISOWeekDate, Time};
use jiff::tz::{AmbiguousOffset, Offset, TimeZone};
use jiff::{SignedDuration, Span, Timestamp, Zoned};

use crate::generators::{self as gs, BoxedGenerator, DefaultGenerator, Generator, Mapped, one_of};

use super::{
    DateGenerator, DateTimeGenerator, OffsetGenerator, SignedDurationGenerator, SpanGenerator,
    TimeGenerator, TimestampGenerator, ZonedGenerator, dates, datetimes, offsets, signed_durations,
    spans, times, timestamps, zoneds,
};

impl DefaultGenerator for Date {
    type Generator = DateGenerator;
    fn default_generator() -> Self::Generator {
        dates()
    }
}

impl DefaultGenerator for Time {
    type Generator = TimeGenerator;
    fn default_generator() -> Self::Generator {
        times()
    }
}

impl DefaultGenerator for DateTime {
    type Generator = DateTimeGenerator;
    fn default_generator() -> Self::Generator {
        datetimes()
    }
}

impl DefaultGenerator for ISOWeekDate {
    type Generator = Mapped<Date, ISOWeekDate, fn(Date) -> ISOWeekDate, DateGenerator>;
    fn default_generator() -> Self::Generator {
        dates().map(|d| d.iso_week_date())
    }
}

impl DefaultGenerator for Timestamp {
    type Generator = TimestampGenerator;
    fn default_generator() -> Self::Generator {
        timestamps()
    }
}

impl DefaultGenerator for Span {
    type Generator = SpanGenerator;
    fn default_generator() -> Self::Generator {
        spans()
    }
}

impl DefaultGenerator for SignedDuration {
    type Generator = SignedDurationGenerator;
    fn default_generator() -> Self::Generator {
        signed_durations()
    }
}

impl DefaultGenerator for Offset {
    type Generator = OffsetGenerator;
    fn default_generator() -> Self::Generator {
        offsets()
    }
}

impl DefaultGenerator for TimeZone {
    type Generator = BoxedGenerator<'static, TimeZone>;
    fn default_generator() -> Self::Generator {
        one_of([
            gs::just::<TimeZone>(TimeZone::UTC).boxed(),
            gs::just::<TimeZone>(TimeZone::unknown()).boxed(),
            offsets().map(TimeZone::fixed).boxed(),
        ])
        .boxed()
    }
}

impl DefaultGenerator for Zoned {
    type Generator = ZonedGenerator;
    fn default_generator() -> Self::Generator {
        zoneds()
    }
}

impl DefaultGenerator for AmbiguousOffset {
    type Generator = BoxedGenerator<'static, AmbiguousOffset>;
    fn default_generator() -> Self::Generator {
        use crate::tuples;
        one_of([
            offsets()
                .map(|offset| AmbiguousOffset::Unambiguous { offset })
                .boxed(),
            tuples!(offsets(), offsets())
                .map(|(before, after)| AmbiguousOffset::Gap { before, after })
                .boxed(),
            tuples!(offsets(), offsets())
                .map(|(before, after)| AmbiguousOffset::Fold { before, after })
                .boxed(),
        ])
        .boxed()
    }
}
