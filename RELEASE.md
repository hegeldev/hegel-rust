RELEASE_TYPE: patch

This patch improves the performance of running tests on the native engine. Hegel no longer formats each drawn value for display unless the output is actually needed — the final replay of a failing example, or verbose mode. Previously every draw on every test case paid this formatting cost even though the rendered text was discarded. The improvement is largest for values that are expensive to format, such as strings and collections.
