RELEASE_TYPE: patch

This patch fixes two related issues with negative zero in the float generator:

- `gs::floats()` previously accepted `min_value=0.0` and `max_value=-0.0` without error. Although `+0.0 == -0.0` under IEEE 754, Hypothesis and the native backend use sign-aware ordering where `-0.0 < +0.0`, so this range contains no valid floats. The generator now panics with a clear `InvalidArgument` message in this case, matching the behaviour of the native backend.
- `HegelValue` was deserializing `-0.0` as `0.0` for float targets. The integer-optimisation branch was triggering for negative zero (because `-0.0.fract() == 0.0`), so the visitor received `visit_i64(0)` and the sign bit was silently lost. This caused `floats(max_value=-0.0)` to generate `0.0` instead of `-0.0`, and made `minimal(floats(), |x| x.is_sign_negative())` shrink to `-1.0` rather than `-0.0`.
