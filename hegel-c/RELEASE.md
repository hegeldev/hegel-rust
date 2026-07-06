RELEASE_TYPE: minor

This release adds inclusive `min_value` / `max_value` bounds to
`hegel_generate_date`, `hegel_generate_time`, and
`hegel_generate_datetime` (a breaking signature change). Pass
`{1, 1, 1}` / `{9999, 12, 31}` and all-zeros / `{23, 59, 59, 999999}`
for the conventional full ranges.

Dates are proleptic Gregorian with `year` in `[-999999, 999999]` and
draw as a single day offset centred on 2000-01-01 (clamped into range),
mirroring Hypothesis's `DateStrategy`, so bounded dates keep the
2000-01-01 shrink target. Times draw as a single microsecond offset
shrinking toward `min_value`, mirroring `TimeStrategy`; previously they
drew four separate components. Datetimes draw a bounded date, then a
time whose bounds tighten to the endpoint times when the drawn date
lands on a boundary date. Invalid calendar dates, out-of-range time
fields, and inverted bounds are rejected with `HEGEL_E_INVALID_ARG`.

Because the underlying choice sequences changed shape, failure
databases and reproduce blobs from earlier versions will not replay
against these draws.
