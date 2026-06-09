RELEASE_TYPE: patch

This patch improves the performance of shrinking a failing test when running with `RUST_BACKTRACE` set (for example, on CI).

Hegel's panic hook previously captured a stack backtrace for every panic raised while running your test body — including the many failing examples explored during shrinking, whose backtraces are never shown. With `RUST_BACKTRACE` set, capturing and symbolizing those discarded backtraces dominated the cost of shrinking a failing property, and was especially slow on Windows. Hegel now captures a backtrace only for a failure it is actually going to report (and, in verbose mode, for each example it prints). The backtrace shown on the reported failure is unchanged.
