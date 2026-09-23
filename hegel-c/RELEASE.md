RELEASE_TYPE: patch

This patch makes opening and closing a span cheaper. A span's label is now recorded as the 64-bit value `hegel_start_span` was given rather than as text, and the engine no longer builds a set of structural-coverage labels for every span, which nothing read. A frontend that opens a span around every draw does noticeably less work per draw as a result.
