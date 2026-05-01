RELEASE_TYPE: patch

This patch fixes `gs::floats()` accepting `min_value=0.0` and `max_value=-0.0` without error. Although `+0.0 == -0.0` under IEEE 754, Hypothesis and the native backend use sign-aware ordering where `-0.0 < +0.0`, so this range contains no valid floats. The generator now panics with a clear `InvalidArgument` message in this case, matching the behaviour of the native backend.
