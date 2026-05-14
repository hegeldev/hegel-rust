RELEASE_TYPE: patch

The native backend (`--features native`) now supports floating-point
generators: `gs::floats::<f32>()` and `gs::floats::<f64>()` work on native,
along with their `min_value` / `max_value` / `exclude_min` / `exclude_max` /
`allow_nan` / `allow_infinity` bounds and the float-specific shrink passes.

`gs::floats()` also now rejects empty-range `exclude_min` / `exclude_max`
combinations (`min_value = +inf` with `exclude_min`, `max_value = -inf` with
`exclude_max`, single-point ranges with either exclude flag) with an
`InvalidArgument` panic at generator construction. Previously the server
backend's Hypothesis layer caught these at draw time; both backends now
agree at builder time.
