RELEASE_TYPE: patch

This patch improves the performance of shrinking: finding the minimal counterexample for a failing test now typically takes around 10x fewer test executions, with unchanged results.
