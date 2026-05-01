RELEASE_TYPE: minor

This release adds the `Phase` enum and `Settings::phases()` API, allowing
callers to control which test lifecycle phases run. The default phase set
is `Explicit`, `Reuse`, `Generate`, `Target`, and `Shrink`.
