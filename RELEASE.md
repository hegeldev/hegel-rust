RELEASE_TYPE: minor

This release deprecates the `exclude_min(bool)` and `exclude_max(bool)` builder methods on `gs::floats()` in favour of `min_value_exclusive` and `max_value_exclusive`, which take the bound directly:

```rust
// before
gs::floats::<f64>().min_value(0.0).exclude_min(true).max_value(1.0).exclude_max(true)

// after
gs::floats::<f64>().min_value_exclusive(0.0).max_value_exclusive(1.0)
```

`min_value` and `min_value_exclusive` set the same bound, so whichever is called last wins (likewise for `max_value` / `max_value_exclusive`). An exclusive bound can no longer be set without a bound value, so that `InvalidArgument` no longer exists; the remaining validation (an exclusive `+inf` minimum, an exclusive `-inf` maximum, or exclusive bounds on a single-point range) is unchanged.

`exclude_min` and `exclude_max` remain as deprecated methods so existing call sites get a deprecation warning naming the replacement, but calling either now panics immediately rather than configuring the generator.
