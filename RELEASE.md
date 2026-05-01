RELEASE_TYPE: minor

This release adds a `no_shrink` setting to `Settings`. When set to `true`, the shrinking phase is skipped even if a failing example is found. This is useful for helpers that just need any witness satisfying a condition, where shrinking would be expensive and unnecessary.
