RELEASE_TYPE: patch

Fix `VecGenerator::unique(true)` silently ignoring the unique constraint when used with composite (non-basic) generators. The fallback draw path now performs client-side deduplication using `collection.reject()`.
