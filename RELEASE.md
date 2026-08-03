RELEASE_TYPE: patch

This patch improves the distribution of `generators::integers` for large and full-width ranges, by way of an engine update. The values that property tests rely on to surface off-by-one, overflow, and sign bugs — the range endpoints and their neighbours, zero, ±1, and small magnitudes — are now drawn much more often, while the middle of the range stays well covered. See the `hegeltest-c` changelog for measurements and details.
