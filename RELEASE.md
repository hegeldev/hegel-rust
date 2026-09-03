RELEASE_TYPE: minor

This release removes the `antithesis` cargo feature. The Antithesis integration is now always compiled in and activates automatically when the `ANTITHESIS_OUTPUT_DIR` environment variable is set, so running inside Antithesis no longer requires a feature flag and no longer fails when the flag is missing. Remove `features = ["antithesis"]` from your `hegeltest` dependency; Cargo rejects unknown features, so builds that still name it will not compile until it is removed.
