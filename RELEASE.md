RELEASE_TYPE: patch

This patch improves the distribution of `generators::integers` for large and full-width ranges, by way of an engine update. The values that property tests rely on to surface off-by-one, overflow, and sign bugs — the range endpoints and their neighbours, zero, ±1, and small magnitudes — are now drawn much more often, while the middle of the range stays well covered. See the `hegeltest-c` changelog for measurements and details.

This patch also fixes the distribution of `generators::floats::<f32>()`, which previously generated infinities on about a third of unbounded draws and produced large finite values essentially never. Unbounded `f32` draws now match the `f64` shape: infinities are rare (about 2%) and large finite magnitudes are common. The underlying clamp fix also affects `f64` generators configured with `allow_nan(false)` or `allow_subnormal(false)`, which previously produced `-inf` on a small fraction of unbounded draws.
