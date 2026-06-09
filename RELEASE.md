RELEASE_TYPE: patch

This patch improves the performance of shrinking a failing test when running with `RUST_BACKTRACE` set. Hegel would previously
capture the stack backtrace for every panic raised while running your test body even when that backtrace was never shown. This could be a significant performance hit, especially on Windows. Hegel now captures only backtraces it needs to print. This should be a significant performance improvement in some workloads, and otherwise have no user-visible effect.
