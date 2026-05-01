RELEASE_TYPE: minor

This release adds `gs::nan_floats()`, a generator for NaN `f64` values with varied sign bits and mantissa bit-patterns. This is a port of Hypothesis's `NanStrategy` and is useful for tests that need to distinguish between different NaN variants.
