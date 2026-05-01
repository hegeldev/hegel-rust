RELEASE_TYPE: minor

This release adds the `Phase` enum and `Settings::phases()` API, allowing
callers to control which test lifecycle phases run. The default phase set
(`Reuse`, `Generate`, `Target`, `Shrink`) matches Hypothesis's defaults.
