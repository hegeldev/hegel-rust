RELEASE_TYPE: patch

This patch tightens argument validation on two C ABI draws so they reject
inconsistent configurations that were previously accepted, matching the checks
the native generator builders already enforce:

- `hegel_generate_float` now returns `HEGEL_E_INVALID_ARG` for `allow_nan=true`
  with a finite `min_value` or `max_value` (which otherwise drew NaN outside the
  stated range), and for `allow_infinity=true` with both bounds finite (a silent
  no-op).
- `hegel_new_collection` now returns `HEGEL_E_INVALID_ARG` when
  `min_size > max_size`, instead of silently accepting the request with undefined
  sizing. Oversized-but-satisfiable requests are still left to the existing
  choice-budget overrun path.
