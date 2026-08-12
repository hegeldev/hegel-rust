RELEASE_TYPE: patch

This patch makes shrinking much cheaper and slightly better: finding the minimal counterexample for a failing test typically takes around 10x fewer test executions, and inputs whose minimum sits in another `one_of`-style branch now reach the true minimal branch more often than before. In our benchmarks every shrunk result is the same or smaller than it was.
