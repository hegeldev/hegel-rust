RELEASE_TYPE: minor

This release changes the default of `Settings::report_multiple_failures` from `true` to `false`. A run that finds several distinct failing origins now reports only one failure, instead of surfacing every origin and panicking with `Property-based test failed with N distinct failures.`. Superficially-distinct failures often share a single root cause, so the collapsed report is the better default.

To restore the previous behavior, opt back in explicitly:

```rust
#[hegel::test(report_multiple_failures = true)]
// or
Settings::new().report_multiple_failures(true)
```
