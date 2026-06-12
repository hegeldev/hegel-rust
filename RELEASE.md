RELEASE_TYPE: minor

This release improves how failing runs are reported, separates "the
property failed" from "the run itself failed", and fixes a bug where
`Verbosity::Quiet` would not always be respected when reporting the
final error.

A failing run now ends by re-raising the failing test's own panic —
payload intact, so `#[should_panic(expected = ...)]` and `catch_unwind`
consumers see exactly what the test raised — instead of a synthetic
`Property test failed: <message>` panic, and the failure no longer prints
a second time after the report. Runs that find several distinct bugs
still fail with the `"... N distinct failures."` message.
