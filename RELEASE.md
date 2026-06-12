RELEASE_TYPE: minor

This release improves how failing runs are reported, separates "the
property failed" from "the run itself failed", and fixes a bug where
`Verbosity::Quiet` would not always be respected when reporting the
final error.
