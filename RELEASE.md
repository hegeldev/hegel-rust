RELEASE_TYPE: patch

This patch improves the performance of running tests. Previously every draw on every test case paid a formatting cost even though the rendered text was discarded, now this formatting is skipped unless the printed result is needed. The improvement is largest for values that are expensive to format, such as strings and collections.
