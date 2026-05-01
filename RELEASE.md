RELEASE_TYPE: patch

This patch fixes `HegelValue` deserializing `-0.0` as `0.0` for float targets. The integer-optimisation branch was triggering for negative zero (because `-0.0.fract() == 0.0`), so the visitor received `visit_i64(0)` and the sign bit was silently lost. This caused `floats(max_value=-0.0)` to generate `0.0` instead of `-0.0`, and made `minimal(floats(), |x| x.is_sign_negative())` shrink to `-1.0` rather than `-0.0`.
