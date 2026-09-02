RELEASE_TYPE: minor

This release makes generated times and datetimes nanosecond resolution instead of microsecond.

`extras::jiff::times()` now generates every `jiff::civil::Time`. A range whose bounds are between two consecutive microseconds is no longer an error.
