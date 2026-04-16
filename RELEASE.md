RELEASE_TYPE: minor

This release adds an experimental native test backend behind the `native` feature flag. When enabled via `--features native`, Hegel runs property-based tests without requiring a Python server, using a pbtkit-style choice-based engine with integrated shrinking.
