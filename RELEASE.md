RELEASE_TYPE: patch

In the native backend (`--features native`), misusing `tc.target()` (a non-finite score, or the same label twice in one test case) now aborts the run with a clear usage error instead of being caught and misreported as a failing example.
