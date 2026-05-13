use chrono::{
    DateTime, Datelike, Days, FixedOffset, IsoWeek, Month, Months, NaiveDate, NaiveDateTime,
    NaiveTime, NaiveWeek, TimeDelta, Utc, Weekday, WeekdaySet,
};

use crate::generators::{
    DefaultGenerator, Generator, IntegerGenerator, JustGenerator, Mapped, integers, just,
};

use super::{
    DateTimeGenerator, FixedOffsetGenerator, NaiveDateGenerator, NaiveDateTimeGenerator,
    NaiveTimeGenerator, NaiveWeekGenerator, TimeDeltaGenerator, WeekdaySetGenerator, datetimes,
    fixed_offsets, naive_dates, naive_datetimes, naive_times, naive_weeks, time_deltas,
    weekday_sets,
};

impl DefaultGenerator for Weekday {
    type Generator = Mapped<u8, Weekday, fn(u8) -> Weekday, IntegerGenerator<u8>>;
    fn default_generator() -> Self::Generator {
        integers::<u8>()
            .min_value(0)
            .max_value(6)
            .map(|n| Weekday::try_from(n).unwrap())
    }
}

impl DefaultGenerator for Month {
    type Generator = Mapped<u8, Month, fn(u8) -> Month, IntegerGenerator<u8>>;
    fn default_generator() -> Self::Generator {
        integers::<u8>()
            .min_value(1)
            .max_value(12)
            .map(|n| Month::try_from(n).unwrap())
    }
}

impl DefaultGenerator for Days {
    type Generator = Mapped<u64, Days, fn(u64) -> Days, IntegerGenerator<u64>>;
    fn default_generator() -> Self::Generator {
        integers::<u64>().map(Days::new)
    }
}

impl DefaultGenerator for Months {
    type Generator = Mapped<u32, Months, fn(u32) -> Months, IntegerGenerator<u32>>;
    fn default_generator() -> Self::Generator {
        integers::<u32>().map(Months::new)
    }
}

impl DefaultGenerator for IsoWeek {
    type Generator = Mapped<NaiveDate, IsoWeek, fn(NaiveDate) -> IsoWeek, NaiveDateGenerator>;
    fn default_generator() -> Self::Generator {
        naive_dates().map(|d| d.iso_week())
    }
}

impl DefaultGenerator for NaiveWeek {
    type Generator = NaiveWeekGenerator<<Weekday as DefaultGenerator>::Generator>;
    fn default_generator() -> Self::Generator {
        naive_weeks()
    }
}

impl DefaultGenerator for WeekdaySet {
    type Generator = WeekdaySetGenerator;
    fn default_generator() -> Self::Generator {
        weekday_sets()
    }
}

impl DefaultGenerator for FixedOffset {
    type Generator = FixedOffsetGenerator;
    fn default_generator() -> Self::Generator {
        fixed_offsets()
    }
}

impl DefaultGenerator for TimeDelta {
    type Generator = TimeDeltaGenerator;
    fn default_generator() -> Self::Generator {
        time_deltas()
    }
}

impl DefaultGenerator for NaiveDate {
    type Generator = NaiveDateGenerator;
    fn default_generator() -> Self::Generator {
        naive_dates()
    }
}

impl DefaultGenerator for NaiveTime {
    type Generator = NaiveTimeGenerator;
    fn default_generator() -> Self::Generator {
        naive_times()
    }
}

impl DefaultGenerator for NaiveDateTime {
    type Generator = NaiveDateTimeGenerator;
    fn default_generator() -> Self::Generator {
        naive_datetimes()
    }
}

impl DefaultGenerator for DateTime<Utc> {
    type Generator = DateTimeGenerator<JustGenerator<Utc>, Utc>;
    fn default_generator() -> Self::Generator {
        datetimes().timezones(just(Utc))
    }
}

impl DefaultGenerator for DateTime<FixedOffset> {
    type Generator = DateTimeGenerator<FixedOffsetGenerator, FixedOffset>;
    fn default_generator() -> Self::Generator {
        datetimes()
    }
}
