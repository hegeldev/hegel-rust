RELEASE_TYPE: minor

This release changes `hegel_time_t` from microsecond to nanosecond resolution. The `microsecond` field (in `[0, 999999]`) is now `nanosecond` (in `[0, 999999999]`). `hegel_generate_time` and `hegel_generate_datetime` now also draw whole nanoseconds.
