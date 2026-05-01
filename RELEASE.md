RELEASE_TYPE: patch

`sampled_from([...]).filter(pred)` now works correctly regardless of how selective the predicate is. Previously, very selective filters (e.g. only one value in 100 satisfies the predicate) would trigger a `FilterTooMuch` health check. Now the filter enumerates the valid subset of elements and picks directly from it. If no element satisfies the predicate, the test panics immediately with a clear "Unsatisfiable filter" message instead of failing via a health check.
